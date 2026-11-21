import { ArrowDownFromLine } from 'lucide-react';
import { api } from '../lib/api';

interface Props {
  frames: number;
  height: number;
  previewUrl: string | null;
}

export default function ScrollProgressCard({ frames, height, previewUrl }: Props) {
  return (
    <aside
      aria-live="polite"
      className="fixed bottom-6 right-6 z-50 w-72 rounded-xl border border-zinc-800 bg-zinc-900/95 p-4 shadow-2xl backdrop-blur"
    >
      <div className="flex items-center justify-between">
        <h2 className="flex items-center gap-2 text-sm font-semibold text-zinc-100">
          <ArrowDownFromLine className="h-4 w-4 text-emerald-400" aria-hidden="true" /> Scrolling
          capture
        </h2>
        <span className="text-xs text-zinc-400">{frames} frames</span>
      </div>
      <p className="mt-1 text-xs text-zinc-400">Stitched: {height}px so far</p>
      {previewUrl && (
        <img
          src={`${previewUrl}?v=${frames}`}
          alt="Stitched capture preview"
          className="mt-3 max-h-40 w-full rounded-md border border-zinc-800 bg-zinc-950 object-contain"
        />
      )}
      <button
        onClick={() => api.cancelScrollCapture().catch(() => {})}
        className="mt-3 w-full rounded-lg border border-zinc-700 px-3 py-2 text-sm text-zinc-200 transition hover:border-emerald-500/60 hover:text-emerald-400"
      >
        Stop and save
      </button>
    </aside>
  );
}
