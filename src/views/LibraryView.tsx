import { convertFileSrc } from '@tauri-apps/api/core';
import { Copy, ExternalLink, FileOutput, FolderOpen, Images, Pencil, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { api } from '../lib/api';
import type { CaptureItem } from '../lib/types';

interface Props {
  items: CaptureItem[];
  onOpenEditor: (item: CaptureItem) => void;
  toast: (text: string, kind?: 'ok' | 'err') => void;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function LibraryView({ items, onOpenEditor, toast }: Props) {
  const [confirmAll, setConfirmAll] = useState(false);

  const act = async (fn: () => Promise<unknown>, ok?: string) => {
    try {
      await fn();
      if (ok) toast(ok);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), 'err');
    }
  };

  const deleteAll = () =>
    act(async () => {
      const n = await api.deleteAllItems();
      setConfirmAll(false);
      toast(`Deleted ${n} item${n === 1 ? '' : 's'}`);
    });

  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-6 sm:px-6">
      <div className="mb-6 flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-lg font-semibold">Library</h1>
        <div className="flex items-center gap-2">
          {items.length > 0 &&
            (confirmAll ? (
              <>
                <span className="text-xs text-zinc-400">
                  Delete all {items.length} item{items.length === 1 ? '' : 's'}? This cannot be undone.
                </span>
                <button
                  onClick={deleteAll}
                  className="flex items-center gap-1.5 rounded-lg border border-red-500/60 bg-red-500/10 px-3 py-1.5 text-sm text-red-400 transition hover:bg-red-500/20"
                >
                  <Trash2 className="h-4 w-4" aria-hidden="true" /> Confirm
                </button>
                <button
                  onClick={() => setConfirmAll(false)}
                  className="rounded-lg border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 transition hover:border-zinc-500"
                >
                  Cancel
                </button>
              </>
            ) : (
              <button
                onClick={() => setConfirmAll(true)}
                className="flex items-center gap-2 rounded-lg border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 transition hover:border-red-500/50 hover:text-red-400"
              >
                <Trash2 className="h-4 w-4" aria-hidden="true" /> Delete all
              </button>
            ))}
          <button
            onClick={() =>
              act(async () => {
                const dir = await api.getLibraryDir();
                await api.openPath(dir);
              })
            }
            className="flex items-center gap-2 rounded-lg border border-zinc-700 px-3 py-1.5 text-sm text-zinc-300 transition hover:border-zinc-500"
          >
            <FolderOpen className="h-4 w-4" aria-hidden="true" /> Open folder
          </button>
        </div>
      </div>

      {items.length === 0 ? (
        <p className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-zinc-800 px-4 py-16 text-center text-sm text-zinc-500">
          <Images className="h-6 w-6" aria-hidden="true" />
          Your capture history is empty. Captures, recordings and GIF exports appear here.
        </p>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {items.map((it) => (
            <article
              key={it.id}
              className="flex max-h-96 flex-col overflow-hidden rounded-xl border border-zinc-800 bg-zinc-900"
            >
              <div className="flex h-44 shrink-0 items-center justify-center overflow-hidden bg-zinc-950">
                {it.kind === 'video' ? (
                  <video
                    src={convertFileSrc(it.path)}
                    controls
                    muted
                    preload="metadata"
                    className="max-h-44 w-full object-contain"
                  />
                ) : (
                  <img
                    src={convertFileSrc(it.path)}
                    alt={`${it.kind} capture`}
                    loading="lazy"
                    className="max-h-44 w-full object-contain"
                  />
                )}
              </div>
              <div className="flex-1 p-4">
                <div className="flex flex-wrap items-center gap-2 text-xs text-zinc-500">
                  <span className="rounded bg-emerald-500/10 px-1.5 py-0.5 font-medium uppercase tracking-wide text-emerald-400">
                    {it.kind}
                  </span>
                  <span>{new Date(it.createdAt).toLocaleString()}</span>
                </div>
                <p className="mt-1 text-sm text-zinc-300">
                  {it.width}x{it.height} - {formatSize(it.sizeBytes)}
                  {it.durationMs ? ` - ${(it.durationMs / 1000).toFixed(1)}s` : ''}
                </p>
              </div>
              <div className="flex flex-wrap gap-1 border-t border-zinc-800 p-3">
                {it.kind === 'image' && (
                  <ActionButton icon={Pencil} label="Edit" onClick={() => onOpenEditor(it)} />
                )}
                {it.kind === 'image' && (
                  <ActionButton
                    icon={Copy}
                    label="Copy"
                    onClick={() => act(() => api.copyImageToClipboard(it.path), 'Image copied to clipboard')}
                  />
                )}
                {it.kind === 'video' && (
                  <ActionButton
                    icon={FileOutput}
                    label="Export GIF"
                    onClick={() => act(() => api.exportGif(it.id), 'GIF export started')}
                  />
                )}
                <ActionButton icon={ExternalLink} label="Open" onClick={() => act(() => api.openPath(it.path))} />
                <ActionButton
                  icon={FolderOpen}
                  label="Reveal"
                  onClick={() => act(() => api.openPath(it.path, true))}
                />
                <ActionButton
                  icon={Trash2}
                  label="Delete"
                  danger
                  onClick={() => act(() => api.deleteItem(it.id), 'Deleted')}
                />
              </div>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}

function ActionButton({
  icon: Icon,
  label,
  onClick,
  danger,
}: {
  icon: typeof Copy;
  label: string;
  onClick: () => void;
  danger?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      title={label}
      aria-label={label}
      className={`flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-xs transition ${
        danger
          ? 'border-zinc-800 text-zinc-400 hover:border-red-500/50 hover:text-red-400'
          : 'border-zinc-800 text-zinc-300 hover:border-emerald-500/50 hover:text-emerald-400'
      }`}
    >
      <Icon className="h-3.5 w-3.5" aria-hidden="true" /> {label}
    </button>
  );
}
