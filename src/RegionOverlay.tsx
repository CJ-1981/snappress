import { useCallback, useEffect, useRef, useState } from 'react';
import { api } from './lib/api';
import type { CaptureMode, Rect } from './lib/types';

/**
 * Fullscreen transparent region-selection overlay.
 * Rendered inside the borderless "overlay" window that covers one monitor.
 * Pointer coordinates are CSS pixels; they are converted to physical pixels
 * with devicePixelRatio before being sent to the Rust backend.
 */
export default function Overlay() {
  const [mode, setMode] = useState<CaptureMode>('image');
  const [displayId, setDisplayId] = useState<number | null>(null);
  const [start, setStart] = useState<{ x: number; y: number } | null>(null);
  const [rect, setRect] = useState<Rect | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const dragging = useRef(false);

  useEffect(() => {
    api.overlayReady()
      .then((r) => {
        setMode(r.mode);
        setDisplayId(r.displayId);
      })
      .catch(() => {});
  }, []);

  const cancel = useCallback(() => {
    if (busy) return;
    api.cancelCapture().catch(() => {});
  }, [busy]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') cancel();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [cancel]);

  const toPhysical = (r: Rect): Rect => {
    const d = window.devicePixelRatio || 1;
    return {
      x: Math.round(r.x * d),
      y: Math.round(r.y * d),
      width: Math.round(r.width * d),
      height: Math.round(r.height * d),
    };
  };

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (busy) return;
    dragging.current = true;
    e.currentTarget.setPointerCapture(e.pointerId);
    const p = { x: e.clientX, y: e.clientY };
    setStart(p);
    setRect({ x: p.x, y: p.y, width: 0, height: 0 });
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragging.current || !start) return;
    setRect({
      x: Math.min(start.x, e.clientX),
      y: Math.min(start.y, e.clientY),
      width: Math.abs(e.clientX - start.x),
      height: Math.abs(e.clientY - start.y),
    });
  };

  const onPointerUp = async () => {
    if (!dragging.current) return;
    dragging.current = false;
    if (!rect) return;
    const p = toPhysical(rect);
    if (p.width < 12 || p.height < 12) {
      // Treat a click without a drag as a cancel.
      setStart(null);
      setRect(null);
      cancel();
      return;
    }
    try {
      if (mode === 'image') {
        setBusy('Capturing region...');
        await api.captureRegion(displayId, p);
      } else if (mode === 'scroll') {
        setBusy('Preparing scrolling capture...');
        const s = await api.getSettings();
        await api.startScrollCapture(displayId, p, {
          delayMs: s.scrollDelayMs,
          stepTicks: s.scrollStepTicks,
          maxHeight: s.maxScrollHeight,
        });
      } else {
        setBusy('Starting recording...');
        await api.startRecording(displayId, p);
      }
      // The backend closes this overlay window once the job is dispatched.
    } catch {
      setBusy(null);
      setStart(null);
      setRect(null);
    }
  };

  const hint = busy
    ? busy
    : mode === 'image'
      ? 'IMAGE: drag to select a region, release to capture'
      : mode === 'scroll'
        ? 'SCROLLING CAPTURE: drag over the scrollable content to capture the whole page'
        : 'RECORD: drag to select the recording area';
  const phys = rect ? toPhysical(rect) : null;

  return (
    <div
      className="fixed inset-0 cursor-crosshair select-none"
      role="application"
      aria-label="Capture region overlay"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
    >
      {rect && rect.width > 2 && rect.height > 2 && (
        <div
          className="absolute border-2 border-emerald-400 bg-emerald-400/10"
          style={{
            left: rect.x,
            top: rect.y,
            width: rect.width,
            height: rect.height,
            boxShadow: '0 0 0 100000px rgba(0,0,0,0.35)',
          }}
        >
          {phys && (
            <div className="absolute -top-7 left-0 rounded bg-emerald-400 px-1.5 py-0.5 font-mono text-[11px] text-zinc-950">
              {phys.width} x {phys.height}
            </div>
          )}
        </div>
      )}
      <div className="pointer-events-none absolute left-1/2 top-6 -translate-x-1/2 rounded-full border border-zinc-700 bg-zinc-900/90 px-4 py-1.5 text-center text-sm text-zinc-200 shadow-lg">
        {hint}
        <span className="text-zinc-500"> - Esc to cancel</span>
      </div>
    </div>
  );
}
