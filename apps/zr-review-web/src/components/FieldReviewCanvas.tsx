import { useEffect, useMemo, useRef, useCallback } from "react";
import type { ReviewFramePayload } from "../lib/types";

const FIELD_GREEN_LIGHT = "#1a5c34";
const FIELD_GREEN_DARK = "#0d3320";
const LINE_COLOR = "#ffffff";
const LINE_GLOW_COLOR = "rgba(255, 255, 255, 0.15)";
const BALL_COLOR = "#ff8c00";
const BALL_GLOW = "rgba(255, 140, 0, 0.4)";
const BLUE_COLOR = "#3b82f6";
const YELLOW_COLOR = "#f59e0b";
const SELECTED_COLOR = "#22d3ee";
const ROBOT_RADIUS_MM = 90;
const BALL_RADIUS_MM = 43;
const PADDING = 40;

// Backend pipeline uses meters; field drawing uses millimeters.
const MM_PER_M = 1000;

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

  const draw = useCallback(() => {
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

    // Deep dark background
    ctx.fillStyle = "#070d15";
    ctx.fillRect(0, 0, w, h);

    if (!allFrames.length && !compareFrame) {
      ctx.fillStyle = "#475569";
      ctx.font = '14px "Inter", system-ui';
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText("No frame data loaded", w / 2, h / 2);
      return;
    }

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

    // --- Field area - gradient fill ---
    const [flx, fly] = toCanvas(-fieldLength / 2, fieldWidth / 2);
    const fieldGrad = ctx.createLinearGradient(flx, fly, flx + fieldLength * scale, fly + fieldWidth * scale);
    fieldGrad.addColorStop(0, FIELD_GREEN_DARK);
    fieldGrad.addColorStop(0.5, FIELD_GREEN_LIGHT);
    fieldGrad.addColorStop(1, FIELD_GREEN_DARK);
    ctx.fillStyle = fieldGrad;
    ctx.fillRect(flx, fly, fieldLength * scale, fieldWidth * scale);

    // --- Subtle grid pattern on field ---
    ctx.strokeStyle = "rgba(255, 255, 255, 0.025)";
    ctx.lineWidth = 1;
    const gridSpacing = 500;
    for (let gx = -fieldLength / 2; gx <= fieldLength / 2; gx += gridSpacing) {
      const [x1, y1] = toCanvas(gx, fieldWidth / 2);
      const [x2, y2] = toCanvas(gx, -fieldWidth / 2);
      ctx.beginPath();
      ctx.moveTo(x1, y1);
      ctx.lineTo(x2, y2);
      ctx.stroke();
    }
    for (let gy = -fieldWidth / 2; gy <= fieldWidth / 2; gy += gridSpacing) {
      const [x1, y1] = toCanvas(-fieldLength / 2, gy);
      const [x2, y2] = toCanvas(fieldLength / 2, gy);
      ctx.beginPath();
      ctx.moveTo(x1, y1);
      ctx.lineTo(x2, y2);
      ctx.stroke();
    }

    // --- Boundary subtle outline ---
    ctx.strokeStyle = "rgba(45, 90, 58, 0.5)";
    ctx.lineWidth = 1;
    const [blx, bly] = toCanvas(-totalLength / 2, totalWidth / 2);
    ctx.strokeRect(blx, bly, totalLength * scale, totalWidth * scale);

    // --- Drawing helpers ---
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

    const drawGlowArc = (cx: number, cy: number, r: number, a1: number, a2: number) => {
      ctx.strokeStyle = LINE_GLOW_COLOR;
      ctx.lineWidth = 6;
      ctx.beginPath();
      ctx.arc(cx, cy, r, a1, a2);
      ctx.stroke();
      ctx.strokeStyle = LINE_COLOR;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.arc(cx, cy, r, a1, a2);
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

    // --- Field lines ---
    const [ox, oy] = toCanvas(-fieldLength / 2, fieldWidth / 2);
    drawGlowRect(ox, oy, fieldLength * scale, fieldWidth * scale);

    // Center line
    const [cx1, cy1] = toCanvas(0, fieldWidth / 2);
    const [cx2, cy2] = toCanvas(0, -fieldWidth / 2);
    drawGlowLine(cx1, cy1, cx2, cy2);

    // Center circle (with glow)
    const [ccx, ccy] = toCanvas(0, 0);
    drawGlowArc(ccx, ccy, 500 * scale, 0, Math.PI * 2);

    // Center dot
    ctx.fillStyle = LINE_COLOR;
    ctx.globalAlpha = 0.6;
    ctx.beginPath();
    ctx.arc(ccx, ccy, 3, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;

    // Goals
    const [lgx, lgy] = toCanvas(-fieldLength / 2 - goalDepth, goalWidth / 2);
    drawGlowRect(lgx, lgy, goalDepth * scale, goalWidth * scale);
    const [rgx, rgy] = toCanvas(fieldLength / 2, goalWidth / 2);
    drawGlowRect(rgx, rgy, goalDepth * scale, goalWidth * scale);

    // --- Draw robot ---
    const drawRobot = (
      robot: { x: number; y: number; theta: number; stable_id: number | null; raw_id: number | null },
      color: string,
      alpha = 1,
      compare = false,
      warning = false,
    ) => {
      // BUG FIX: Convert from meters (backend) to millimeters (field drawing)
      const [rx, ry] = toCanvas(robot.x * MM_PER_M, robot.y * MM_PER_M);
      const r = Math.max(ROBOT_RADIUS_MM * scale, 8);
      ctx.globalAlpha = alpha;

      // Outer glow / shadow (skip for compare/trail ghosts)
      if (!compare && alpha > 0.5) {
        const glowGrad = ctx.createRadialGradient(rx, ry, r, rx, ry, r * 2.5);
        glowGrad.addColorStop(0, color === BLUE_COLOR ? "rgba(59,130,246,0.15)" : "rgba(245,158,11,0.15)");
        glowGrad.addColorStop(1, "rgba(0,0,0,0)");
        ctx.fillStyle = glowGrad;
        ctx.beginPath();
        ctx.arc(rx, ry, r * 2.5, 0, Math.PI * 2);
        ctx.fill();
      }

      // Warning ring (identity swap)
      if (warning && alpha > 0.5) {
        ctx.beginPath();
        ctx.arc(rx, ry, r + 5, 0, Math.PI * 2);
        ctx.strokeStyle = SELECTED_COLOR;
        ctx.lineWidth = 2;
        ctx.stroke();
        ctx.beginPath();
        ctx.arc(rx, ry, r + 9, 0, Math.PI * 2);
        ctx.strokeStyle = "rgba(34, 211, 238, 0.3)";
        ctx.lineWidth = 1;
        ctx.stroke();
      }

      // Robot body with gradient
      const bodyGrad = ctx.createRadialGradient(rx - r * 0.3, ry - r * 0.3, 0, rx, ry, r);
      if (color === BLUE_COLOR) {
        bodyGrad.addColorStop(0, "#60a5fa");
        bodyGrad.addColorStop(1, "#2563eb");
      } else if (color === YELLOW_COLOR) {
        bodyGrad.addColorStop(0, "#fbbf24");
        bodyGrad.addColorStop(1, "#d97706");
      } else {
        bodyGrad.addColorStop(0, color);
        bodyGrad.addColorStop(1, color);
      }
      ctx.beginPath();
      ctx.arc(rx, ry, r, 0, Math.PI * 2);
      ctx.fillStyle = bodyGrad;
      ctx.fill();

      // Border
      ctx.strokeStyle = compare ? "rgba(255,255,255,0.35)" : "rgba(255,255,255,0.6)";
      ctx.lineWidth = compare ? 1 : 1.5;
      ctx.stroke();

      // Orientation arrow
      const dirLen = r + 8;
      const tipX = rx + Math.cos(robot.theta) * dirLen;
      const tipY = ry - Math.sin(robot.theta) * dirLen;

      ctx.beginPath();
      ctx.moveTo(rx, ry);
      ctx.lineTo(tipX, tipY);
      ctx.strokeStyle = color === BLUE_COLOR ? "#93c5fd" : color === YELLOW_COLOR ? "#fcd34d" : "#ffffff";
      ctx.lineWidth = 2.5;
      ctx.stroke();

      // Arrow tip dot
      ctx.beginPath();
      ctx.arc(tipX, tipY, 2.5, 0, Math.PI * 2);
      ctx.fillStyle = "#ffffff";
      ctx.fill();

      // Robot ID with dark outline for readability
      const fontSize = Math.max(r * 0.85, 9);
      ctx.font = `bold ${fontSize}px "Inter", system-ui`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.lineJoin = "round";
      // Dark outline
      ctx.strokeStyle = "rgba(0, 0, 0, 0.7)";
      ctx.lineWidth = 3;
      ctx.strokeText(String(robot.stable_id ?? robot.raw_id ?? "?"), rx, ry);
      // White fill
      ctx.fillStyle = "#ffffff";
      ctx.fillText(String(robot.stable_id ?? robot.raw_id ?? "?"), rx, ry);

      ctx.globalAlpha = 1;
    };

    // --- Draw trail + current frames ---
    for (const historyFrame of allFrames) {
      const alpha = historyFrame === frame ? 1 : 0.18;
      for (const robot of historyFrame.target_team) {
        drawRobot(robot, BLUE_COLOR, alpha, false, showIdentityOverlay && historyFrame.flags.likely_identity_swap);
      }
      for (const robot of historyFrame.opponent_team) {
        drawRobot(robot, YELLOW_COLOR, alpha, false, showIdentityOverlay && historyFrame.flags.likely_identity_swap);
      }
      if (historyFrame.ball) {
        // BUG FIX: Convert from meters to millimeters
        const [bx, by] = toCanvas(historyFrame.ball.x * MM_PER_M, historyFrame.ball.y * MM_PER_M);
        const r = Math.max(BALL_RADIUS_MM * scale, 5);
        ctx.globalAlpha = alpha;

        // Ball glow (only for current frame)
        if (alpha > 0.5) {
          const glowGrad = ctx.createRadialGradient(bx, by, r * 0.5, bx, by, r * 4);
          glowGrad.addColorStop(0, BALL_GLOW);
          glowGrad.addColorStop(1, "rgba(255, 140, 0, 0)");
          ctx.fillStyle = glowGrad;
          ctx.beginPath();
          ctx.arc(bx, by, r * 4, 0, Math.PI * 2);
          ctx.fill();
        }

        // Ball body with radial gradient
        const ballGrad = ctx.createRadialGradient(bx - r * 0.3, by - r * 0.3, 0, bx, by, r);
        ballGrad.addColorStop(0, "#ffb347");
        ballGrad.addColorStop(0.7, BALL_COLOR);
        ballGrad.addColorStop(1, "#cc6600");
        ctx.beginPath();
        ctx.arc(bx, by, r, 0, Math.PI * 2);
        ctx.fillStyle = ballGrad;
        ctx.fill();
        ctx.strokeStyle = "rgba(255, 255, 255, 0.5)";
        ctx.lineWidth = 1;
        ctx.stroke();

        ctx.globalAlpha = 1;
      }
    }

    // --- Compare (raw) overlay ---
    if (showCompare && compareFrame) {
      // The cleaning pipeline normalises coordinates so the target team
      // always attacks positive-x.  Raw frames may still be in the
      // original orientation.  When they differ we need to mirror the raw
      // positions so both overlays share the same coordinate system.
      const needFlip =
        frame != null &&
        frame.target_attacks_positive_x !== compareFrame.target_attacks_positive_x;
      const maybeFlip = (robot: { x: number; y: number; theta: number; stable_id: number | null; raw_id: number | null }) =>
        needFlip
          ? { ...robot, x: -robot.x, y: -robot.y, theta: robot.theta + Math.PI }
          : robot;

      for (const robot of compareFrame.target_team) {
        drawRobot(maybeFlip(robot), "rgba(56,189,248,0.6)", 0.5, true, false);
      }
      for (const robot of compareFrame.opponent_team) {
        drawRobot(maybeFlip(robot), "rgba(251,191,36,0.6)", 0.5, true, false);
      }
    }

    // --- Live overlay ---
    if (showLiveOverlay && frame) {
      ctx.fillStyle = frame.live ? "rgba(16,185,129,0.18)" : "rgba(239,68,68,0.18)";
      ctx.fillRect(0, 0, w, h);
    }

    // --- Field dimensions label ---
    ctx.fillStyle = "rgba(71, 85, 105, 0.7)";
    ctx.font = '10px "JetBrains Mono", monospace';
    ctx.textAlign = "left";
    ctx.textBaseline = "bottom";
    ctx.fillText(`${fieldLength / 1000}m \u00d7 ${fieldWidth / 1000}m`, PADDING, h - 10);
  }, [allFrames, compareFrame, frame, showCompare, showIdentityOverlay, showLiveOverlay]);

  // Redraw when data changes
  useEffect(() => {
    draw();
  }, [draw]);

  // Resize observer — redraw when container size changes
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver(() => draw());
    observer.observe(container);
    return () => observer.disconnect();
  }, [draw]);

  return (
    <div ref={containerRef} className="w-full h-full relative overflow-hidden rounded-xl">
      <canvas ref={canvasRef} className="absolute inset-0" />
    </div>
  );
}
