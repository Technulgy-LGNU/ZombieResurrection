import { useEffect, useMemo, useRef } from "react";
import type { ReviewFramePayload } from "../lib/types";

const FIELD_GREEN_LIGHT = "#1a5c34";
const FIELD_GREEN_DARK = "#0d3320";
const LINE_COLOR = "#ffffff";
const LINE_GLOW_COLOR = "rgba(255, 255, 255, 0.15)";
const BALL_COLOR = "#ff8c00";
const BLUE_COLOR = "#3b82f6";
const YELLOW_COLOR = "#f59e0b";
const SELECTED_COLOR = "#22d3ee";
const ROBOT_RADIUS_MM = 90;
const BALL_RADIUS_MM = 43;
const PADDING = 40;

interface Props {
  frame: ReviewFramePayload | null;
  compareFrame: ReviewFramePayload | null;
  showCompare: boolean;
  showTrails: boolean;
  trailFrames: ReviewFramePayload[];
  showIdentityOverlay: boolean;
  showLiveOverlay: boolean;
}

export default function FieldReviewCanvas({
  frame,
  compareFrame,
  showCompare,
  showTrails,
  trailFrames,
  showIdentityOverlay,
  showLiveOverlay,
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const allFrames = useMemo(() => {
    const frames = showTrails ? trailFrames : [];
    return frame ? [...frames, frame] : frames;
  }, [frame, showTrails, trailFrames]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) return;

    const rect = container.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    canvas.style.width = `${rect.width}px`;
    canvas.style.height = `${rect.height}px`;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const w = rect.width;
    const h = rect.height;
    ctx.fillStyle = "#070d15";
    ctx.fillRect(0, 0, w, h);

    const fieldLength = 9000;
    const fieldWidth = 6000;
    const boundaryWidth = 300;
    const goalDepth = 180;
    const goalWidth = 1000;
    const totalLength = fieldLength + 2 * boundaryWidth;
    const totalWidth = fieldWidth + 2 * boundaryWidth;
    const scale = Math.min((w - 2 * PADDING) / totalLength, (h - 2 * PADDING) / totalWidth);
    const offsetX = w / 2;
    const offsetY = h / 2;
    const toCanvas = (fx: number, fy: number): [number, number] => [offsetX + fx * scale, offsetY - fy * scale];

    const [flx, fly] = toCanvas(-fieldLength / 2, fieldWidth / 2);
    const fieldGrad = ctx.createLinearGradient(flx, fly, flx + fieldLength * scale, fly + fieldWidth * scale);
    fieldGrad.addColorStop(0, FIELD_GREEN_DARK);
    fieldGrad.addColorStop(0.5, FIELD_GREEN_LIGHT);
    fieldGrad.addColorStop(1, FIELD_GREEN_DARK);
    ctx.fillStyle = fieldGrad;
    ctx.fillRect(flx, fly, fieldLength * scale, fieldWidth * scale);

    const drawGlowLine = (x1: number, y1: number, x2: number, y2: number) => {
      ctx.strokeStyle = LINE_GLOW_COLOR;
      ctx.lineWidth = 6;
      ctx.beginPath();
      ctx.moveTo(x1, y1);
      ctx.lineTo(x2, y2);
      ctx.stroke();
      ctx.strokeStyle = LINE_COLOR;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(x1, y1);
      ctx.lineTo(x2, y2);
      ctx.stroke();
    };

    const drawGlowRect = (x: number, y: number, rw: number, rh: number) => {
      ctx.strokeStyle = LINE_GLOW_COLOR;
      ctx.lineWidth = 6;
      ctx.strokeRect(x, y, rw, rh);
      ctx.strokeStyle = LINE_COLOR;
      ctx.lineWidth = 1.5;
      ctx.strokeRect(x, y, rw, rh);
    };

    const [ox, oy] = toCanvas(-fieldLength / 2, fieldWidth / 2);
    drawGlowRect(ox, oy, fieldLength * scale, fieldWidth * scale);
    const [cx1, cy1] = toCanvas(0, fieldWidth / 2);
    const [cx2, cy2] = toCanvas(0, -fieldWidth / 2);
    drawGlowLine(cx1, cy1, cx2, cy2);
    const [ccx, ccy] = toCanvas(0, 0);
    ctx.strokeStyle = LINE_COLOR;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.arc(ccx, ccy, 500 * scale, 0, Math.PI * 2);
    ctx.stroke();
    const [lgx, lgy] = toCanvas(-fieldLength / 2 - goalDepth, goalWidth / 2);
    drawGlowRect(lgx, lgy, goalDepth * scale, goalWidth * scale);
    const [rgx, rgy] = toCanvas(fieldLength / 2, goalWidth / 2);
    drawGlowRect(rgx, rgy, goalDepth * scale, goalWidth * scale);

    const drawRobot = (
      robot: { x: number; y: number; theta: number; stable_id: number | null; raw_id: number | null },
      color: string,
      alpha = 1,
      compare = false,
      warning = false,
    ) => {
      const [rx, ry] = toCanvas(robot.x * 1000, robot.y * 1000);
      const r = Math.max(ROBOT_RADIUS_MM * scale, 8);
      ctx.globalAlpha = alpha;
      ctx.beginPath();
      ctx.arc(rx, ry, r, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.fill();
      ctx.strokeStyle = warning ? SELECTED_COLOR : compare ? "rgba(255,255,255,0.35)" : "rgba(255,255,255,0.7)";
      ctx.lineWidth = compare ? 1 : 2;
      ctx.stroke();
      const tipX = rx + Math.cos(robot.theta) * (r + 8);
      const tipY = ry - Math.sin(robot.theta) * (r + 8);
      ctx.beginPath();
      ctx.moveTo(rx, ry);
      ctx.lineTo(tipX, tipY);
      ctx.strokeStyle = "#ffffff";
      ctx.lineWidth = 2;
      ctx.stroke();
      ctx.fillStyle = "#ffffff";
      ctx.font = `bold ${Math.max(r * 0.8, 9)}px Inter`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(String(robot.stable_id ?? robot.raw_id ?? "?"), rx, ry);
      ctx.globalAlpha = 1;
    };

    for (const historyFrame of allFrames) {
      const alpha = historyFrame === frame ? 1 : 0.18;
      for (const robot of historyFrame.target_team) {
        drawRobot(robot, BLUE_COLOR, alpha, false, showIdentityOverlay && historyFrame.flags.likely_identity_swap);
      }
      for (const robot of historyFrame.opponent_team) {
        drawRobot(robot, YELLOW_COLOR, alpha, false, showIdentityOverlay && historyFrame.flags.likely_identity_swap);
      }
      if (historyFrame.ball) {
        const [bx, by] = toCanvas(historyFrame.ball.x * 1000, historyFrame.ball.y * 1000);
        const r = Math.max(BALL_RADIUS_MM * scale, 5);
        ctx.globalAlpha = alpha;
        ctx.beginPath();
        ctx.arc(bx, by, r, 0, Math.PI * 2);
        ctx.fillStyle = BALL_COLOR;
        ctx.fill();
        ctx.globalAlpha = 1;
      }
    }

    if (showCompare && compareFrame) {
      for (const robot of compareFrame.target_team) {
        drawRobot(robot, "rgba(56,189,248,0.6)", 0.5, true, false);
      }
      for (const robot of compareFrame.opponent_team) {
        drawRobot(robot, "rgba(251,191,36,0.6)", 0.5, true, false);
      }
    }

    if (showLiveOverlay && frame) {
      ctx.fillStyle = frame.live ? "rgba(16,185,129,0.18)" : "rgba(239,68,68,0.18)";
      ctx.fillRect(0, 0, w, h);
    }
  }, [allFrames, compareFrame, frame, showCompare, showIdentityOverlay, showLiveOverlay]);

  return (
    <div ref={containerRef} className="w-full h-full relative overflow-hidden rounded-xl">
      <canvas ref={canvasRef} className="absolute inset-0" />
    </div>
  );
}
