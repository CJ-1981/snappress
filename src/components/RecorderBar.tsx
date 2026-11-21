import { Pause, Play, Square } from 'lucide-react';
import { api } from '../lib/api';

function formatMs(ms: number): string {
  const total = Math.floor(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

export default function RecorderBar({ elapsedMs, paused }: { elapsedMs: number; paused: boolean }) {
  return (
    <div
      role="region"
      aria-label="Recording controls"
      className="fixed bottom-6 left-1/2 z-50 flex -translate-x-1/2 items-center gap-4 rounded-full border border-zinc-800 bg-zinc-900/95 px-5 py-2.5 shadow-2xl backdrop-blur"
    >
      <span className="flex items-center gap-2">
        <span
          className={`h-2.5 w-2.5 rounded-full bg-red-500 ${paused ? '' : 'animate-pulse'}`}
          aria-hidden="true"
        />
        <span className="font-mono text-sm tabular-nums text-zinc-100">
          {paused ? 'PAUSED ' : ''}
          {formatMs(elapsedMs)}
        </span>
      </span>
      <button
        onClick={() => api.pauseRecording(!paused).catch(() => {})}
        aria-label={paused ? 'Resume recording' : 'Pause recording'}
        title={paused ? 'Resume recording' : 'Pause recording'}
        className="rounded-full p-2 text-zinc-300 transition hover:bg-zinc-800 hover:text-emerald-400"
      >
        {paused ? <Play className="h-4 w-4" aria-hidden="true" /> : <Pause className="h-4 w-4" aria-hidden="true" />}
      </button>
      <button
        onClick={() => api.stopRecording().catch(() => {})}
        aria-label="Stop recording"
        className="flex items-center gap-1.5 rounded-full bg-red-500 px-3 py-1.5 text-xs font-medium text-white transition hover:bg-red-400"
      >
        <Square className="h-3.5 w-3.5" aria-hidden="true" /> Stop
      </button>
    </div>
  );
}
