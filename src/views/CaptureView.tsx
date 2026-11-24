import { convertFileSrc } from '@tauri-apps/api/core';
import {
  AlertTriangle,
  ArrowDownFromLine,
  Crop,
  Images,
  Maximize,
  Monitor,
  Video,
  X,
} from 'lucide-react';
import { useState } from 'react';
import { api } from '../lib/api';
import type { CaptureItem, DisplayInfo } from '../lib/types';

interface Props {
  displays: DisplayInfo[];
  displayId: number | null;
  onSelectDisplay: (id: number | null) => void;
  items: CaptureItem[];
  ffmpegReady: boolean | null;
  onOpenEditor: (item: CaptureItem) => void;
  /** Last failure reported by the backend, until dismissed. */
  captureError: string | null;
  onDismissCaptureError: () => void;
}

export default function CaptureView({
  displays,
  displayId,
  onSelectDisplay,
  items,
  ffmpegReady,
  onOpenEditor,
  captureError,
  onDismissCaptureError,
}: Props) {
  const [error, setError] = useState<string | null>(null);

  const run = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div className="mx-auto w-full max-w-5xl px-4 py-6 sm:px-6">
      <div className="mb-6 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h1 className="text-lg font-semibold">Capture</h1>
        <label className="flex items-center gap-2 text-sm text-zinc-400">
          <Monitor className="h-4 w-4" aria-hidden="true" />
          <span className="sr-only">Target display</span>
          <select
            value={displayId ?? ''}
            onChange={(e) => onSelectDisplay(e.target.value ? Number(e.target.value) : null)}
            className="rounded-md border border-zinc-800 bg-zinc-900 px-2.5 py-1.5 text-sm text-zinc-200 focus:border-emerald-500/60 focus:outline-none"
          >
            {displays.length === 0 && <option value="">No displays detected</option>}
            {displays.map((d) => (
              <option key={d.id} value={d.id}>
                {d.name} ({d.width}x{d.height}){d.isPrimary ? ' - primary' : ''}
              </option>
            ))}
          </select>
        </label>
      </div>

      {ffmpegReady === false && (
        <div className="mb-6 flex items-start gap-3 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-300">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
          <p>
            ffmpeg is not installed yet. The first recording or GIF export will download it
            automatically (about 80 MB).
          </p>
        </div>
      )}

      {(error || captureError) && (
        <div
          role="alert"
          className="mb-6 flex items-start justify-between gap-3 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-300"
        >
          <p className="min-w-0 break-words">{error ?? captureError}</p>
          <button
            onClick={() => {
              setError(null);
              onDismissCaptureError();
            }}
            aria-label="Dismiss error"
            className="shrink-0 rounded p-1 text-red-300/70 transition hover:bg-red-500/10 hover:text-red-200"
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>
      )}

      <div className="grid gap-4 md:grid-cols-3">
        <section className="flex flex-col rounded-xl border border-zinc-800 bg-zinc-900 p-6 transition hover:border-emerald-500/40">
          <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-400">
            <Crop className="h-5 w-5" aria-hidden="true" />
          </div>
          <h2 className="text-base font-semibold">Region capture</h2>
          <p className="mb-4 mt-1 flex-1 text-sm text-zinc-400">
            Drag on screen to select any region and save it as a PNG. Esc cancels the overlay.
          </p>
          <button
            onClick={() => run(() => api.startRegionCapture('image', displayId))}
            className="rounded-lg bg-emerald-500 px-3 py-2 text-sm font-medium text-zinc-950 transition hover:bg-emerald-400"
          >
            Capture region
          </button>
          <button
            onClick={() => run(() => api.captureFullScreen(displayId))}
            className="mt-2 flex items-center justify-center gap-2 rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-300 transition hover:border-zinc-500"
          >
            <Maximize className="h-4 w-4" aria-hidden="true" /> Full screen
          </button>
        </section>

        <section className="flex flex-col rounded-xl border border-zinc-800 bg-zinc-900 p-6 transition hover:border-emerald-500/40">
          <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-400">
            <ArrowDownFromLine className="h-5 w-5" aria-hidden="true" />
          </div>
          <h2 className="text-base font-semibold">Scrolling capture</h2>
          <p className="mb-4 mt-1 flex-1 text-sm text-zinc-400">
            Automatically scrolls the page and smart-stitches everything into one tall image.
            Delay and step size are in Settings.
          </p>
          <button
            onClick={() => run(() => api.startRegionCapture('scroll', displayId))}
            className="rounded-lg bg-emerald-500 px-3 py-2 text-sm font-medium text-zinc-950 transition hover:bg-emerald-400"
          >
            Scrolling capture
          </button>
        </section>

        <section className="flex flex-col rounded-xl border border-zinc-800 bg-zinc-900 p-6 transition hover:border-emerald-500/40">
          <div className="mb-3 flex h-10 w-10 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-400">
            <Video className="h-5 w-5" aria-hidden="true" />
          </div>
          <h2 className="text-base font-semibold">Record screen</h2>
          <p className="mb-4 mt-1 flex-1 text-sm text-zinc-400">
            Records the selected region to an H.264 MP4 video. GIF export is available from the
            library afterwards.
          </p>
          <button
            onClick={() => run(() => api.startRegionCapture('record', displayId))}
            className="rounded-lg bg-emerald-500 px-3 py-2 text-sm font-medium text-zinc-950 transition hover:bg-emerald-400"
          >
            Record screen
          </button>
        </section>
      </div>

      <section className="mt-8">
        <h2 className="mb-3 flex items-center gap-2 text-sm font-medium text-zinc-400">
          <Images className="h-4 w-4" aria-hidden="true" /> Recent captures
        </h2>
        {items.length === 0 ? (
          <p className="rounded-lg border border-dashed border-zinc-800 px-4 py-8 text-center text-sm text-zinc-500">
            Nothing captured yet. Use a mode above or the global hotkey.
          </p>
        ) : (
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 lg:grid-cols-6">
            {items.slice(0, 12).map((it) => (
              <button
                key={it.id}
                onClick={() => it.kind === 'image' && onOpenEditor(it)}
                title={it.kind === 'image' ? 'Open in editor' : 'Video capture'}
                className="group overflow-hidden rounded-lg border border-zinc-800 bg-zinc-900 text-left transition hover:border-emerald-500/40"
              >
                {it.kind === 'video' ? (
                  <video
                    src={convertFileSrc(it.path)}
                    className="h-24 w-full object-cover"
                    muted
                    preload="metadata"
                  />
                ) : (
                  <img
                    src={convertFileSrc(it.path)}
                    alt={`${it.kind} capture thumbnail`}
                    className="h-24 w-full object-cover"
                    loading="lazy"
                  />
                )}
                <div className="truncate px-2 py-1.5 text-[11px] text-zinc-400">
                  {it.width}x{it.height} - {it.kind}
                </div>
              </button>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
