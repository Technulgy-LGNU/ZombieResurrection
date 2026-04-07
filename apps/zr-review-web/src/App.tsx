import { useEffect, useMemo, useState } from "react";
import FieldReviewCanvas from "./components/FieldReviewCanvas";
import { fetchGame, fetchGames, fetchSequence, updateReview } from "./lib/api";
import type {
  GameListItem,
  ReviewGamePayload,
  ReviewSequenceListItem,
  ReviewSequencePayload,
  ReviewVerdict,
} from "./lib/types";

const verdictOrder: ReviewVerdict[] = ["Unreviewed", "NeedsAttention", "Drop", "Keep"];

/** Format a unix-epoch timestamp (seconds) into HH:MM:SS.mmm */
function fmtTime(epochS: number, precision: 1 | 2 | 3 = 3): string {
  const d = new Date(epochS * 1000);
  const hh = String(d.getUTCHours()).padStart(2, "0");
  const mm = String(d.getUTCMinutes()).padStart(2, "0");
  const ss = String(d.getUTCSeconds()).padStart(2, "0");
  const frac = epochS.toFixed(precision).split(".")[1];
  return `${hh}:${mm}:${ss}.${frac}`;
}

function verdictDot(verdict: ReviewVerdict) {
  switch (verdict) {
    case "Keep":
      return "bg-emerald-400 shadow-[0_0_6px_rgba(52,211,153,0.5)]";
    case "NeedsAttention":
      return "bg-amber-400 shadow-[0_0_6px_rgba(251,191,36,0.5)]";
    case "Drop":
      return "bg-red-400 shadow-[0_0_6px_rgba(248,113,113,0.5)]";
    default:
      return "bg-slate-500";
  }
}

function verdictBtnClass(verdict: ReviewVerdict, active: boolean) {
  if (!active) return "bg-slate-800/80 text-slate-200 hover:bg-slate-700/80";
  switch (verdict) {
    case "Keep":
      return "btn-verdict-keep text-white";
    case "NeedsAttention":
      return "btn-verdict-attention text-slate-900";
    case "Drop":
      return "btn-verdict-drop text-white";
    default:
      return "btn-glow text-slate-950";
  }
}

function verdictLabel(verdict: ReviewVerdict) {
  switch (verdict) {
    case "NeedsAttention":
      return "Attention";
    default:
      return verdict;
  }
}

export default function App() {
  const [games, setGames] = useState<GameListItem[]>([]);
  const [selectedPath, setSelectedPath] = useState<string>("");
  const [game, setGame] = useState<ReviewGamePayload | null>(null);
  const [selectedSequenceId, setSelectedSequenceId] = useState<number | null>(null);
  const [selectedSequence, setSelectedSequence] = useState<ReviewSequencePayload | null>(null);
  const [frameIndex, setFrameIndex] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [showCompare, setShowCompare] = useState(true);
  const [showTrails, setShowTrails] = useState(true);
  const [showIdentityOverlay, setShowIdentityOverlay] = useState(true);
  const [showLiveOverlay, setShowLiveOverlay] = useState(false);
  const [filterVerdict, setFilterVerdict] = useState<ReviewVerdict | "All">("All");
  const [filterText, setFilterText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loadingSequence, setLoadingSequence] = useState(false);
  const [playWholeGame, setPlayWholeGame] = useState(false);

  useEffect(() => {
    fetchGames()
      .then((items) => {
        setGames(items);
        if (items[0]) setSelectedPath(items[0].path);
      })
      .catch((err) => setError(String(err)));
  }, []);

  useEffect(() => {
    if (!selectedPath) return;
    setError(null);
    setGame(null);
    setSelectedSequence(null);
    setSelectedSequenceId(null);
    setFrameIndex(0);
    setPlayWholeGame(false);
    setPlaying(false);
    fetchGame(selectedPath)
      .then((payload) => {
        setGame(payload);
        setSelectedSequenceId(payload.sequences[0]?.summary.sequence_index ?? null);
      })
      .catch((err) => setError(String(err)));
  }, [selectedPath]);

  const filteredSequences = useMemo(() => {
    const sequences = game?.sequences ?? [];
    return sequences.filter((sequence) => {
      const verdictOk = filterVerdict === "All" || sequence.verdict === filterVerdict;
      const text = `${sequence.summary.sequence_kind} ${sequence.summary.warnings.join(" ")} ${sequence.note}`.toLowerCase();
      const textOk = filterText.trim() === "" || text.includes(filterText.toLowerCase());
      return verdictOk && textOk;
    });
  }, [filterText, filterVerdict, game?.sequences]);

  useEffect(() => {
    if (!filteredSequences.length) {
      setSelectedSequenceId(null);
      setSelectedSequence(null);
      return;
    }
    if (selectedSequenceId !== null && filteredSequences.some((sequence) => sequence.summary.sequence_index === selectedSequenceId)) {
      return;
    }
    setSelectedSequenceId(filteredSequences[0].summary.sequence_index);
  }, [filteredSequences, selectedSequenceId]);

  useEffect(() => {
    if (!selectedPath || selectedSequenceId === null) {
      setSelectedSequence(null);
      return;
    }
    setLoadingSequence(true);
    setFrameIndex(0);
    fetchSequence(selectedPath, selectedSequenceId)
      .then((payload) => setSelectedSequence(payload.sequence))
      .catch((err) => setError(String(err)))
      .finally(() => setLoadingSequence(false));
  }, [selectedPath, selectedSequenceId]);

  useEffect(() => {
    const lastFrame = Math.max((selectedSequence?.cleaned_frames.length ?? 1) - 1, 0);
    setFrameIndex((current) => Math.min(current, lastFrame));
  }, [selectedSequence]);

  useEffect(() => {
    if (!playing || !selectedSequence || loadingSequence) return;
    const timer = window.setInterval(() => {
      setFrameIndex((index) => {
        const last = selectedSequence.cleaned_frames.length - 1;
        if (index >= last) return playWholeGame ? index : 0;
        return index + 1;
      });
    }, 100);
    return () => window.clearInterval(timer);
  }, [playing, selectedSequence, playWholeGame, loadingSequence]);

  // Auto-advance to next sequence in whole-game playback mode
  useEffect(() => {
    if (!playing || !playWholeGame || !selectedSequence || loadingSequence) return;
    const last = selectedSequence.cleaned_frames.length - 1;
    if (frameIndex < last) return;
    const currentIdx = filteredSequences.findIndex(
      (s) => s.summary.sequence_index === selectedSequenceId,
    );
    if (currentIdx < 0 || currentIdx >= filteredSequences.length - 1) {
      // Reached the end of all sequences
      setPlaying(false);
      setPlayWholeGame(false);
      return;
    }
    setSelectedSequenceId(filteredSequences[currentIdx + 1].summary.sequence_index);
  }, [playing, playWholeGame, selectedSequence, loadingSequence, frameIndex, filteredSequences, selectedSequenceId]);

  const selectedSummary: ReviewSequenceListItem | null = useMemo(
    () => game?.sequences.find((sequence) => sequence.summary.sequence_index === selectedSequenceId) ?? null,
    [game?.sequences, selectedSequenceId],
  );

  const currentCleaned = selectedSequence?.cleaned_frames[frameIndex] ?? null;
  const currentRaw = selectedSequence?.raw_frames[frameIndex] ?? null;
  const trailFrames = selectedSequence?.cleaned_frames.slice(Math.max(0, frameIndex - 10), frameIndex + 1) ?? [];

  async function setVerdict(verdict: ReviewVerdict) {
    if (!game || !selectedSummary) return;
    const note = selectedSummary.note;
    await updateReview({
      game_id: game.game_id,
      sequence_index: selectedSummary.summary.sequence_index,
      verdict,
      note,
    });
    setGame({
      ...game,
      sequences: game.sequences.map((sequence) =>
        sequence.summary.sequence_index === selectedSummary.summary.sequence_index
          ? { ...sequence, verdict }
          : sequence,
      ),
    });
    setSelectedSequence((current) => (current ? { ...current, verdict } : current));
  }

  async function setNote(note: string) {
    if (!game || !selectedSummary) return;
    const verdict = selectedSummary.verdict;
    setGame({
      ...game,
      sequences: game.sequences.map((sequence) =>
        sequence.summary.sequence_index === selectedSummary.summary.sequence_index
          ? { ...sequence, note }
          : sequence,
      ),
    });
    setSelectedSequence((current) => (current ? { ...current, note } : current));
    await updateReview({
      game_id: game.game_id,
      sequence_index: selectedSummary.summary.sequence_index,
      verdict,
      note,
    });
  }

  return (
    <div className="h-full flex flex-col bg-dot-pattern text-slate-100">
      {/* ===== Header ===== */}
      <header className="flex items-center justify-between px-5 py-2.5 bg-slate-900/80 backdrop-blur-xl border-b border-slate-700/40 shrink-0 relative">
        <div className="absolute bottom-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-cyan-500/30 to-transparent" />
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2.5">
            <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-cyan-500 to-blue-600 flex items-center justify-center shadow-lg shadow-cyan-500/20">
              <svg className="w-4 h-4 text-white" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
              </svg>
            </div>
            <h1 className="text-lg font-bold tracking-tight">
              <span className="text-cyan-400">Zombie</span>
              <span className="text-slate-200">Review</span>
            </h1>
          </div>
          <div className="h-4 w-px bg-slate-700/60" />
          <span className="text-xs text-slate-500 font-mono tracking-wide">Dataset Review Workstation</span>
        </div>
        <div className="flex items-center gap-3">
          <select
            value={selectedPath}
            onChange={(e) => setSelectedPath(e.target.value)}
            className="glass-panel focus-cyan px-3 py-2 text-sm min-w-[420px] cursor-pointer"
          >
            {games.map((gameItem) => (
              <option key={gameItem.path} value={gameItem.path}>{gameItem.path}</option>
            ))}
          </select>
        </div>
      </header>

      {/* ===== Main content ===== */}
      <div className="flex-1 min-h-0 flex gap-2 p-2">
        {/* --- Left sidebar: Sequence list --- */}
        <aside className="w-[360px] shrink-0 glass-panel panel-accent flex flex-col overflow-hidden">
          <div className="p-3 border-b border-slate-700/30">
            <h2 className="text-[10px] font-semibold text-cyan-400/80 uppercase tracking-[0.15em] mb-2 flex items-center gap-1.5">
              <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 20V10" />
                <path d="M18 20V4" />
                <path d="M6 20v-4" />
              </svg>
              Sequences
            </h2>
            <div className="flex gap-2 mb-2">
              <input
                value={filterText}
                onChange={(e) => setFilterText(e.target.value)}
                placeholder="Filter warnings / notes"
                className="flex-1 rounded-lg bg-slate-900/60 border border-slate-700/50 px-3 py-2 text-sm focus-cyan transition-all duration-200"
              />
              <select
                value={filterVerdict}
                onChange={(e) => setFilterVerdict(e.target.value as ReviewVerdict | "All")}
                className="rounded-lg bg-slate-900/60 border border-slate-700/50 px-3 py-2 text-sm focus-cyan cursor-pointer"
              >
                <option>All</option>
                {verdictOrder.map((value) => <option key={value}>{value}</option>)}
              </select>
            </div>
            <div className="text-[11px] font-mono text-slate-500">
              {filteredSequences.length} sequence{filteredSequences.length !== 1 ? "s" : ""}
              {game ? ` of ${game.sequences.length}` : ""}
            </div>
          </div>
          <div className="flex-1 overflow-y-auto p-2 space-y-1.5">
            {filteredSequences.map((sequence) => {
              const isSelected = selectedSequenceId === sequence.summary.sequence_index;
              return (
                <button
                  key={sequence.summary.sequence_index}
                  onClick={() => setSelectedSequenceId(sequence.summary.sequence_index)}
                  className={`w-full text-left rounded-xl border px-3 py-3 hover-lift transition-all duration-200 ${
                    isSelected
                      ? "border-cyan-400/60 bg-cyan-500/10 shadow-[0_0_15px_rgba(6,182,212,0.08)]"
                      : "border-slate-700/40 bg-slate-900/40 hover:border-slate-600/50"
                  }`}
                >
                  <div className="flex items-center justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-xs text-slate-500">#{sequence.summary.sequence_index}</span>
                      <span className="text-sm font-semibold">{sequence.summary.sequence_kind}</span>
                    </div>
                    <div className="flex items-center gap-1.5">
                      <span className={`inline-block w-2 h-2 rounded-full ${verdictDot(sequence.verdict)}`} />
                      <span className="text-[10px] uppercase tracking-[0.12em] text-slate-400">{verdictLabel(sequence.verdict)}</span>
                    </div>
                  </div>
                  <div className="mt-1.5 flex items-center gap-3 text-xs text-slate-400 font-mono">
                    <span>{sequence.summary.frame_count}f</span>
                    <span className="text-slate-600">&middot;</span>
                    <span>q={sequence.summary.quality_score.toFixed(2)}</span>
                    <span className="text-slate-600">&middot;</span>
                    <span>{fmtTime(sequence.summary.start_time_s, 1)}&ndash;{fmtTime(sequence.summary.end_time_s, 1)}</span>
                  </div>
                  {/* Quality bar */}
                  <div className="mt-2 h-1 rounded-full bg-slate-800/80 overflow-hidden">
                    <div
                      className="h-full rounded-full bg-gradient-to-r from-cyan-500/60 to-cyan-400/80"
                      style={{ width: `${Math.min(sequence.summary.quality_score * 100, 100)}%` }}
                    />
                  </div>
                  {sequence.summary.warnings.length > 0 && (
                    <div className="mt-2 flex flex-wrap gap-1">
                      {sequence.summary.warnings.slice(0, 3).map((warning) => (
                        <span
                          key={warning}
                          className="rounded-full bg-amber-500/10 border border-amber-500/20 px-2 py-0.5 text-[10px] text-amber-300"
                        >
                          {warning}
                        </span>
                      ))}
                      {sequence.summary.warnings.length > 3 && (
                        <span className="rounded-full bg-slate-800/80 px-2 py-0.5 text-[10px] text-slate-400">
                          +{sequence.summary.warnings.length - 3}
                        </span>
                      )}
                    </div>
                  )}
                </button>
              );
            })}
            {filteredSequences.length === 0 && (
              <div className="p-6 text-center animate-fade-in">
                <p className="text-sm text-slate-500">No sequences match filters</p>
              </div>
            )}
          </div>
        </aside>

        {/* --- Center: Field visualization --- */}
        <main className="flex-1 min-w-0 flex flex-col gap-2">
          <div className="flex-1 glass-panel panel-accent overflow-hidden min-h-0">
            <div className="h-full flex flex-col">
              {/* Toolbar */}
              <div className="px-4 py-3 border-b border-slate-700/30 flex items-center justify-between gap-3">
                <div>
                  <div className="text-sm font-semibold flex items-center gap-2">
                    {game?.target_team}
                    <span className="text-slate-500 font-normal">vs</span>
                    {game?.opponent_team}
                    {game && (
                      <span className={`ml-1 inline-block w-2.5 h-2.5 rounded-full ${
                        game.target_color === "Blue"
                          ? "bg-blue-500 shadow-[0_0_6px_rgba(59,130,246,0.5)]"
                          : "bg-amber-500 shadow-[0_0_6px_rgba(245,158,11,0.5)]"
                      }`} />
                    )}
                  </div>
                  <div className="text-xs text-slate-400 mt-0.5">
                    {game?.phase}
                    {selectedSummary && (
                      <>
                        <span className="mx-1.5 text-slate-600">&middot;</span>
                        <span className="font-mono">{fmtTime(selectedSummary.summary.start_time_s, 2)} &ndash; {fmtTime(selectedSummary.summary.end_time_s, 2)}</span>
                      </>
                    )}
                    {!selectedSummary && " \u2013 No sequence selected"}
                  </div>
                </div>
                <div className="flex items-center gap-2 text-xs">
                  <button
                    onClick={() => {
                      if (playWholeGame) {
                        setPlayWholeGame(false);
                        setPlaying(false);
                      } else {
                        setPlayWholeGame(true);
                        setPlaying(true);
                      }
                    }}
                    className={`rounded-lg px-4 py-2 font-semibold transition-all duration-200 ${
                      playWholeGame
                        ? "bg-gradient-to-r from-rose-500 to-pink-600 text-white shadow-lg shadow-rose-500/20"
                        : "bg-gradient-to-r from-emerald-500 to-cyan-500 text-slate-950 shadow-lg shadow-cyan-500/20"
                    }`}
                    disabled={!game || filteredSequences.length === 0}
                  >
                    {playWholeGame ? "Stop All" : "Play All"}
                  </button>
                  <button
                    onClick={() => setPlaying((value) => !value)}
                    className="btn-glow rounded-lg px-4 py-2 font-semibold text-slate-950"
                    disabled={!selectedSequence}
                  >
                    {playing ? "Pause" : "Play"}
                  </button>
                  <button
                    onClick={() => setFrameIndex((value) => Math.max(0, value - 1))}
                    className="rounded-lg bg-slate-800/80 border border-slate-700/40 px-3 py-2 hover:bg-slate-700/80 transition-all duration-200"
                    disabled={!selectedSequence}
                  >
                    Prev
                  </button>
                  <button
                    onClick={() => setFrameIndex((value) => Math.min((selectedSequence?.cleaned_frames.length ?? 1) - 1, value + 1))}
                    className="rounded-lg bg-slate-800/80 border border-slate-700/40 px-3 py-2 hover:bg-slate-700/80 transition-all duration-200"
                    disabled={!selectedSequence}
                  >
                    Next
                  </button>
                </div>
              </div>

              {/* Canvas */}
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

              {/* Playback controls */}
              <div className="px-4 py-3 border-t border-slate-700/30 space-y-3">
                <input
                  type="range"
                  min={0}
                  max={Math.max((selectedSequence?.cleaned_frames.length ?? 1) - 1, 0)}
                  value={frameIndex}
                  onChange={(e) => setFrameIndex(Number(e.target.value))}
                  className="w-full"
                  disabled={!selectedSequence}
                />
                <div className="flex items-center justify-between text-xs text-slate-400 font-mono">
                  <span>
                    {loadingSequence
                      ? "Loading sequence..."
                      : selectedSequence
                        ? `Frame ${frameIndex + 1} / ${selectedSequence.cleaned_frames.length}${
                            playWholeGame
                              ? ` \u00b7 Seq ${filteredSequences.findIndex((s) => s.summary.sequence_index === selectedSequenceId) + 1} / ${filteredSequences.length}`
                              : ""
                          }`
                        : "No frames loaded"}
                  </span>
                  <span>{selectedSequence?.cleaned_frames[frameIndex] ? fmtTime(selectedSequence.cleaned_frames[frameIndex].timestamp_s) : "-"}</span>
                </div>

                <div className="h-px bg-gradient-to-r from-transparent via-slate-700/40 to-transparent" />

                <div className="flex flex-wrap items-center gap-4 text-xs text-slate-300">
                  <label className="flex items-center gap-2 cursor-pointer hover:text-slate-100 transition-colors">
                    <input type="checkbox" checked={showCompare} onChange={(e) => setShowCompare(e.target.checked)} className="accent-cyan-400" />
                    Raw compare
                  </label>
                  <label className="flex items-center gap-2 cursor-pointer hover:text-slate-100 transition-colors">
                    <input type="checkbox" checked={showTrails} onChange={(e) => setShowTrails(e.target.checked)} className="accent-cyan-400" />
                    Trails
                  </label>
                  <label className="flex items-center gap-2 cursor-pointer hover:text-slate-100 transition-colors">
                    <input type="checkbox" checked={showIdentityOverlay} onChange={(e) => setShowIdentityOverlay(e.target.checked)} className="accent-cyan-400" />
                    Identity overlay
                  </label>
                  <label className="flex items-center gap-2 cursor-pointer hover:text-slate-100 transition-colors">
                    <input type="checkbox" checked={showLiveOverlay} onChange={(e) => setShowLiveOverlay(e.target.checked)} className="accent-cyan-400" />
                    Live overlay
                  </label>
                </div>
              </div>
            </div>
          </div>
        </main>

        {/* --- Right sidebar: Review panel --- */}
        <aside className="w-[360px] shrink-0 glass-panel panel-accent flex flex-col overflow-hidden">
          <div className="p-3 border-b border-slate-700/30">
            <h2 className="text-[10px] font-semibold text-cyan-400/80 uppercase tracking-[0.15em] flex items-center gap-1.5">
              <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M9 11l3 3L22 4" />
                <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
              </svg>
              Review
            </h2>
          </div>
          <div className="flex-1 overflow-y-auto p-3 space-y-4">
            {/* Verdict buttons */}
            <div>
              <div className="mb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-slate-400">Verdict</div>
              <div className="grid grid-cols-3 gap-2">
                {(["Keep", "NeedsAttention", "Drop"] as ReviewVerdict[]).map((verdict) => (
                  <button
                    key={verdict}
                    onClick={() => void setVerdict(verdict)}
                    className={`rounded-lg px-3 py-2.5 text-sm font-semibold transition-all duration-200 ${verdictBtnClass(verdict, selectedSummary?.verdict === verdict)}`}
                    disabled={!selectedSummary}
                  >
                    {verdictLabel(verdict)}
                  </button>
                ))}
              </div>
            </div>

            <div className="h-px bg-gradient-to-r from-transparent via-slate-700/40 to-transparent" />

            {/* Warnings */}
            <div>
              <div className="mb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-slate-400 flex items-center gap-1.5">
                <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
                  <line x1="12" y1="9" x2="12" y2="13" />
                  <line x1="12" y1="17" x2="12.01" y2="17" />
                </svg>
                Warnings
              </div>
              {selectedSequence?.warnings.length ? (
                <div className="flex flex-wrap gap-2">
                  {selectedSequence.warnings.map((warning) => (
                    <span key={warning} className="rounded-full bg-amber-500/10 px-2.5 py-1 text-xs text-amber-300 border border-amber-500/20">
                      {warning}
                    </span>
                  ))}
                </div>
              ) : (
                <p className="text-xs text-slate-500 italic">No warnings</p>
              )}
            </div>

            <div className="h-px bg-gradient-to-r from-transparent via-slate-700/40 to-transparent" />

            {/* Notes */}
            <div>
              <div className="mb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-slate-400 flex items-center gap-1.5">
                <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                  <polyline points="14 2 14 8 20 8" />
                  <line x1="16" y1="13" x2="8" y2="13" />
                  <line x1="16" y1="17" x2="8" y2="17" />
                </svg>
                Notes
              </div>
              <textarea
                value={selectedSequence?.note ?? ""}
                onChange={(e) => void setNote(e.target.value)}
                placeholder="Add review notes..."
                className="min-h-[140px] w-full rounded-xl border border-slate-700/50 bg-slate-950/60 p-3 text-sm text-slate-100 placeholder-slate-600 focus-cyan transition-all duration-200 resize-y"
                disabled={!selectedSummary}
              />
            </div>

            <div className="h-px bg-gradient-to-r from-transparent via-slate-700/40 to-transparent" />

            {/* Audit */}
            <div>
              <div className="mb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-slate-400 flex items-center gap-1.5">
                <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="12" cy="12" r="10" />
                  <line x1="12" y1="16" x2="12" y2="12" />
                  <line x1="12" y1="8" x2="12.01" y2="8" />
                </svg>
                Audit
              </div>
              {game?.audit_notes.length ? (
                <div className="space-y-1.5 text-xs text-slate-300">
                  {game.audit_notes.map((note) => (
                    <div key={note} className="rounded-lg bg-slate-900/50 border border-slate-700/30 px-3 py-2">
                      {note}
                    </div>
                  ))}
                </div>
              ) : (
                <p className="text-xs text-slate-500 italic">No audit notes</p>
              )}
            </div>

            {/* Frame flags (current frame) */}
            {currentCleaned && (
              <>
                <div className="h-px bg-gradient-to-r from-transparent via-slate-700/40 to-transparent" />
                <div>
                  <div className="mb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-slate-400 flex items-center gap-1.5">
                    <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z" />
                      <line x1="4" y1="22" x2="4" y2="15" />
                    </svg>
                    Frame Flags
                  </div>
                  <div className="grid grid-cols-2 gap-x-3 gap-y-1.5 text-[11px] font-mono">
                    <span className="text-slate-500">Live</span>
                    <span className={`text-right ${currentCleaned.live ? "text-emerald-400" : "text-slate-500"}`}>
                      {currentCleaned.live ? "yes" : "no"}
                    </span>
                    <span className="text-slate-500">Ref live</span>
                    <span className={`text-right ${currentCleaned.flags.referee_live ? "text-emerald-400" : "text-slate-500"}`}>
                      {currentCleaned.flags.referee_live ? "yes" : "no"}
                    </span>
                    <span className="text-slate-500">Heur live</span>
                    <span className={`text-right ${currentCleaned.flags.heuristic_live ? "text-emerald-400" : "text-slate-500"}`}>
                      {currentCleaned.flags.heuristic_live ? "yes" : "no"}
                    </span>
                    <span className="text-slate-500">Dup ts</span>
                    <span className={`text-right ${currentCleaned.flags.duplicate_timestamp ? "text-amber-400" : "text-slate-500"}`}>
                      {currentCleaned.flags.duplicate_timestamp ? "yes" : "no"}
                    </span>
                    <span className="text-slate-500">Carried</span>
                    <span className={`text-right ${currentCleaned.flags.carried_ball ? "text-amber-400" : "text-slate-500"}`}>
                      {currentCleaned.flags.carried_ball ? "yes" : "no"}
                    </span>
                    <span className="text-slate-500">ID swap</span>
                    <span className={`text-right ${currentCleaned.flags.likely_identity_swap ? "text-red-400" : "text-slate-500"}`}>
                      {currentCleaned.flags.likely_identity_swap ? "yes" : "no"}
                    </span>
                  </div>
                </div>
              </>
            )}
          </div>
        </aside>
      </div>

      {/* ===== Error banner ===== */}
      {error && (
        <div className="mx-2 mb-2 rounded-xl border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-200 flex items-center gap-3 animate-fade-in">
          <svg className="w-4 h-4 shrink-0 text-red-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="10" />
            <line x1="15" y1="9" x2="9" y2="15" />
            <line x1="9" y1="9" x2="15" y2="15" />
          </svg>
          <span className="flex-1">{error}</span>
          <button onClick={() => setError(null)} className="text-red-400 hover:text-red-300 text-xs">
            Dismiss
          </button>
        </div>
      )}
    </div>
  );
}
