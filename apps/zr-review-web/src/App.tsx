import { useEffect, useMemo, useState } from "react";
import FieldReviewCanvas from "./components/FieldReviewCanvas";
import { fetchGame, fetchGames, updateReview } from "./lib/api";
import type { GameListItem, ReviewGamePayload, ReviewSequencePayload, ReviewVerdict } from "./lib/types";

const verdictOrder: ReviewVerdict[] = ["Unreviewed", "NeedsAttention", "Drop", "Keep"];

export default function App() {
  const [games, setGames] = useState<GameListItem[]>([]);
  const [selectedPath, setSelectedPath] = useState<string>("");
  const [game, setGame] = useState<ReviewGamePayload | null>(null);
  const [selectedSequenceIndex, setSelectedSequenceIndex] = useState(0);
  const [frameIndex, setFrameIndex] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [showCompare, setShowCompare] = useState(true);
  const [showTrails, setShowTrails] = useState(true);
  const [showIdentityOverlay, setShowIdentityOverlay] = useState(true);
  const [showLiveOverlay, setShowLiveOverlay] = useState(false);
  const [filterVerdict, setFilterVerdict] = useState<ReviewVerdict | "All">("All");
  const [filterText, setFilterText] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchGames().then((items) => {
      setGames(items);
      if (items[0]) setSelectedPath(items[0].path);
    }).catch((err) => setError(String(err)));
  }, []);

  useEffect(() => {
    if (!selectedPath) return;
    setError(null);
    fetchGame(selectedPath).then((payload) => {
      setGame(payload);
      setSelectedSequenceIndex(0);
      setFrameIndex(0);
    }).catch((err) => setError(String(err)));
  }, [selectedPath]);

  const sequences = game?.sequences ?? [];
  const filteredSequences = useMemo(() => {
    return sequences.filter((sequence) => {
      const verdictOk = filterVerdict === "All" || sequence.verdict === filterVerdict;
      const text = `${sequence.summary.sequence_kind} ${sequence.warnings.join(" ")} ${sequence.note}`.toLowerCase();
      const textOk = filterText.trim() === "" || text.includes(filterText.toLowerCase());
      return verdictOk && textOk;
    });
  }, [filterText, filterVerdict, sequences]);

  const selectedSequence = filteredSequences[selectedSequenceIndex] ?? filteredSequences[0] ?? null;

  useEffect(() => {
    setFrameIndex(0);
  }, [selectedSequenceIndex, selectedPath]);

  useEffect(() => {
    if (!playing || !selectedSequence) return;
    const timer = window.setInterval(() => {
      setFrameIndex((index) => {
        const last = selectedSequence.cleaned_frames.length - 1;
        return index >= last ? 0 : index + 1;
      });
    }, 100);
    return () => window.clearInterval(timer);
  }, [playing, selectedSequence]);

  const currentCleaned = selectedSequence?.cleaned_frames[frameIndex] ?? null;
  const currentRaw = selectedSequence?.raw_frames[frameIndex] ?? null;
  const trailFrames = selectedSequence?.cleaned_frames.slice(Math.max(0, frameIndex - 10), frameIndex + 1) ?? [];

  async function setVerdict(verdict: ReviewVerdict) {
    if (!game || !selectedSequence) return;
    const note = selectedSequence.note;
    await updateReview({
      game_id: game.game_id,
      sequence_index: selectedSequence.summary.sequence_index,
      verdict,
      note,
    });
    setGame({
      ...game,
      sequences: game.sequences.map((sequence) =>
        sequence.summary.sequence_index === selectedSequence.summary.sequence_index
          ? { ...sequence, verdict }
          : sequence,
      ),
    });
  }

  async function setNote(note: string) {
    if (!game || !selectedSequence) return;
    const verdict = selectedSequence.verdict;
    setGame({
      ...game,
      sequences: game.sequences.map((sequence) =>
        sequence.summary.sequence_index === selectedSequence.summary.sequence_index
          ? { ...sequence, note }
          : sequence,
      ),
    });
    await updateReview({
      game_id: game.game_id,
      sequence_index: selectedSequence.summary.sequence_index,
      verdict,
      note,
    });
  }

  return (
    <div className="h-full flex flex-col bg-dot-pattern text-slate-100">
      <header className="flex items-center justify-between px-5 py-2.5 bg-slate-900/80 backdrop-blur-xl border-b border-slate-700/40 shrink-0 relative">
        <div className="absolute bottom-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-cyan-500/30 to-transparent" />
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2.5">
            <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-cyan-500 to-blue-600 flex items-center justify-center shadow-lg shadow-cyan-500/20" />
            <h1 className="text-lg font-bold tracking-tight"><span className="text-cyan-400">Zombie</span><span className="text-slate-200">Review</span></h1>
          </div>
          <div className="h-4 w-px bg-slate-700/60" />
          <span className="text-xs text-slate-500 font-mono tracking-wide">Dataset Review Workstation</span>
        </div>
        <div className="flex items-center gap-3">
          <select value={selectedPath} onChange={(e) => setSelectedPath(e.target.value)} className="glass-panel px-3 py-2 text-sm min-w-[420px]">
            {games.map((game) => <option key={game.path} value={game.path}>{game.path}</option>)}
          </select>
        </div>
      </header>

      <div className="flex-1 min-h-0 flex gap-2 p-2">
        <aside className="w-[360px] shrink-0 glass-panel panel-accent flex flex-col overflow-hidden">
          <div className="p-3 border-b border-slate-700/30">
            <div className="flex gap-2 mb-2">
              <input value={filterText} onChange={(e) => setFilterText(e.target.value)} placeholder="Filter warnings / notes" className="flex-1 rounded-lg bg-slate-900/60 border border-slate-700/50 px-3 py-2 text-sm" />
              <select value={filterVerdict} onChange={(e) => setFilterVerdict(e.target.value as ReviewVerdict | "All")} className="rounded-lg bg-slate-900/60 border border-slate-700/50 px-3 py-2 text-sm">
                <option>All</option>
                {verdictOrder.map((value) => <option key={value}>{value}</option>)}
              </select>
            </div>
            <div className="text-xs text-slate-400">{filteredSequences.length} sequences</div>
          </div>
          <div className="flex-1 overflow-y-auto p-2 space-y-2">
            {filteredSequences.map((sequence, index) => (
              <button key={sequence.summary.sequence_index} onClick={() => setSelectedSequenceIndex(index)} className={`w-full text-left rounded-xl border px-3 py-3 hover-lift ${selectedSequence?.summary.sequence_index === sequence.summary.sequence_index ? "border-cyan-400/60 bg-cyan-500/10" : "border-slate-700/40 bg-slate-900/40"}`}>
                <div className="flex items-center justify-between gap-2">
                  <div className="text-sm font-semibold">#{sequence.summary.sequence_index} {sequence.summary.sequence_kind}</div>
                  <span className="text-[10px] uppercase tracking-[0.15em] text-slate-400">{sequence.verdict}</span>
                </div>
                <div className="mt-1 text-xs text-slate-400">{sequence.summary.frame_count}f • q={sequence.summary.quality_score.toFixed(2)}</div>
                <div className="mt-2 flex flex-wrap gap-1">
                  {sequence.warnings.slice(0, 3).map((warning) => <span key={warning} className="rounded-full bg-slate-800/80 px-2 py-0.5 text-[10px] text-amber-300">{warning}</span>)}
                </div>
              </button>
            ))}
          </div>
        </aside>

        <main className="flex-1 min-w-0 flex flex-col gap-2">
          <div className="flex-1 glass-panel panel-accent overflow-hidden min-h-0">
            <div className="h-full flex flex-col">
              <div className="px-4 py-3 border-b border-slate-700/30 flex items-center justify-between gap-3">
                <div>
                  <div className="text-sm font-semibold">{game?.target_team} vs {game?.opponent_team}</div>
                  <div className="text-xs text-slate-400">{game?.phase} • {selectedSequence?.summary.start_time_s.toFixed(2)}s to {selectedSequence?.summary.end_time_s.toFixed(2)}s</div>
                </div>
                <div className="flex items-center gap-2 text-xs">
                  <button onClick={() => setPlaying((value) => !value)} className="btn-glow rounded-lg px-3 py-2 font-semibold text-slate-950">{playing ? "Pause" : "Play"}</button>
                  <button onClick={() => setFrameIndex((value) => Math.max(0, value - 1))} className="rounded-lg bg-slate-800/80 px-3 py-2">Prev</button>
                  <button onClick={() => setFrameIndex((value) => Math.min((selectedSequence?.cleaned_frames.length ?? 1) - 1, value + 1))} className="rounded-lg bg-slate-800/80 px-3 py-2">Next</button>
                </div>
              </div>
              <div className="flex-1 min-h-0 p-3">
                <FieldReviewCanvas
                  frame={currentCleaned}
                  compareFrame={currentRaw}
                  showCompare={showCompare}
                  showTrails={showTrails}
                  trailFrames={trailFrames}
                  showIdentityOverlay={showIdentityOverlay}
                  showLiveOverlay={showLiveOverlay}
                />
              </div>
              <div className="px-4 py-3 border-t border-slate-700/30 space-y-3">
                <input type="range" min={0} max={Math.max((selectedSequence?.cleaned_frames.length ?? 1) - 1, 0)} value={frameIndex} onChange={(e) => setFrameIndex(Number(e.target.value))} className="w-full accent-cyan-400" />
                <div className="flex flex-wrap items-center gap-3 text-xs text-slate-300">
                  <label className="flex items-center gap-2"><input type="checkbox" checked={showCompare} onChange={(e) => setShowCompare(e.target.checked)} /> Raw compare</label>
                  <label className="flex items-center gap-2"><input type="checkbox" checked={showTrails} onChange={(e) => setShowTrails(e.target.checked)} /> Trails</label>
                  <label className="flex items-center gap-2"><input type="checkbox" checked={showIdentityOverlay} onChange={(e) => setShowIdentityOverlay(e.target.checked)} /> Identity overlay</label>
                  <label className="flex items-center gap-2"><input type="checkbox" checked={showLiveOverlay} onChange={(e) => setShowLiveOverlay(e.target.checked)} /> Live overlay</label>
                </div>
              </div>
            </div>
          </div>
        </main>

        <aside className="w-[360px] shrink-0 glass-panel panel-accent flex flex-col overflow-hidden">
          <div className="p-3 border-b border-slate-700/30">
            <div className="text-[10px] font-semibold text-cyan-400/80 uppercase tracking-[0.15em]">Review</div>
          </div>
          <div className="flex-1 overflow-y-auto p-3 space-y-4">
            <div className="grid grid-cols-3 gap-2">
              {(["Keep", "NeedsAttention", "Drop"] as ReviewVerdict[]).map((verdict) => (
                <button key={verdict} onClick={() => void setVerdict(verdict)} className={`rounded-lg px-3 py-2 text-sm font-semibold ${selectedSequence?.verdict === verdict ? "btn-glow text-slate-950" : "bg-slate-800/80 text-slate-200"}`}>{verdict}</button>
              ))}
            </div>

            <div>
              <div className="mb-2 text-xs uppercase tracking-[0.15em] text-slate-400">Warnings</div>
              <div className="flex flex-wrap gap-2">
                {selectedSequence?.warnings.map((warning) => <span key={warning} className="rounded-full bg-amber-500/10 px-2 py-1 text-xs text-amber-300 border border-amber-500/20">{warning}</span>)}
              </div>
            </div>

            <div>
              <div className="mb-2 text-xs uppercase tracking-[0.15em] text-slate-400">Notes</div>
              <textarea value={selectedSequence?.note ?? ""} onChange={(e) => void setNote(e.target.value)} className="min-h-[160px] w-full rounded-xl border border-slate-700/50 bg-slate-950/60 p-3 text-sm text-slate-100" />
            </div>

            <div>
              <div className="mb-2 text-xs uppercase tracking-[0.15em] text-slate-400">Audit</div>
              <div className="space-y-2 text-xs text-slate-300">
                {game?.audit_notes.map((note) => <div key={note} className="rounded-lg bg-slate-900/50 px-3 py-2">{note}</div>)}
              </div>
            </div>
          </div>
        </aside>
      </div>

      {error && <div className="mx-2 mb-2 rounded-xl border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-200">{error}</div>}
    </div>
  );
}
