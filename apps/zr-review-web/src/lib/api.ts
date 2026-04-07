import type { GameListItem, ReviewGamePayload, ReviewSequenceQueryPayload, ReviewVerdict } from "./types";

export async function fetchGames(): Promise<GameListItem[]> {
  const response = await fetch("/api/games");
  if (!response.ok) throw new Error(await response.text());
  return response.json();
}

export async function fetchGame(path: string): Promise<ReviewGamePayload> {
  const params = new URLSearchParams({ path });
  const response = await fetch(`/api/game?${params.toString()}`);
  if (!response.ok) throw new Error(await response.text());
  return response.json();
}

export async function fetchSequence(path: string, sequenceIndex: number): Promise<ReviewSequenceQueryPayload> {
  const params = new URLSearchParams({ path, sequence_index: String(sequenceIndex) });
  const response = await fetch(`/api/sequence?${params.toString()}`);
  if (!response.ok) throw new Error(await response.text());
  return response.json();
}

export async function updateReview(payload: {
  game_id: string;
  sequence_index: number;
  verdict: ReviewVerdict;
  note: string;
}) {
  const response = await fetch("/api/review", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!response.ok) throw new Error(await response.text());
  return response.json();
}
