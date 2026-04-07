export type ReviewVerdict = "Keep" | "Drop" | "NeedsAttention" | "Unreviewed";

export interface GameListItem {
  id: string;
  path: string;
}

export interface ReviewRobotPayload {
  slot: number;
  raw_id: number | null;
  stable_id: number | null;
  role: string;
  x: number;
  y: number;
  theta: number;
  vx: number;
  vy: number;
}

export interface ReviewBallPayload {
  x: number;
  y: number;
  vx: number;
  vy: number;
}

export interface ReviewFlagsPayload {
  duplicate_timestamp: boolean;
  carried_ball: boolean;
  likely_identity_swap: boolean;
  referee_live: boolean;
  heuristic_live: boolean;
}

export interface ReviewFramePayload {
  timestamp_s: number;
  frame_number: number;
  live: boolean;
  target_attacks_positive_x: boolean;
  target_team: ReviewRobotPayload[];
  opponent_team: ReviewRobotPayload[];
  ball: ReviewBallPayload | null;
  flags: ReviewFlagsPayload;
}

export interface ReviewSequenceSummary {
  sequence_index: number;
  start_frame: number;
  end_frame: number;
  start_time_s: number;
  end_time_s: number;
  frame_count: number;
  quality_score: number;
  sequence_kind: string;
  warnings: string[];
}

export interface ReviewSequenceListItem {
  summary: ReviewSequenceSummary;
  verdict: ReviewVerdict;
  note: string;
}

export interface ReviewSequencePayload {
  summary: ReviewSequenceSummary;
  verdict: ReviewVerdict;
  note: string;
  warnings: string[];
  cleaned_frames: ReviewFramePayload[];
  raw_frames: ReviewFramePayload[];
}

export interface ReviewGamePayload {
  game_id: string;
  source_log: string;
  target_team: string;
  opponent_team: string;
  target_color: "Yellow" | "Blue";
  phase: string;
  audit_notes: string[];
  sequences: ReviewSequenceListItem[];
}

export interface ReviewSequenceQueryPayload {
  game_id: string;
  source_log: string;
  sequence: ReviewSequencePayload;
}
