import { convertFileSrc } from '@tauri-apps/api/core';
import { Aperture, Images, Scan, Settings } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import RecorderBar from './components/RecorderBar';
import ScrollProgressCard from './components/ScrollProgressCard';
import SettingsDialog from './components/SettingsDialog';
import CaptureView from './views/CaptureView';
import EditorView from './views/EditorView';
import LibraryView from './views/LibraryView';
import { api, onEvent } from './lib/api';
import type { AppSettings, CaptureItem, DisplayInfo } from './lib/types';

type View = 'capture' | 'library' | 'editor';

interface Toast {
  id: number;
  text: string;
  kind: 'ok' | 'err';
}

export default function App() {
  const [view, setView] = useState<View>('capture');
  const [items, setItems] = useState<CaptureItem[]>([]);
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [displayId, setDisplayId] = useState<number | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [editorItem, setEditorItem] = useState<CaptureItem | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [ffmpegReady, setFfmpegReady] = useState<boolean | null>(null);
  const [rec, setRec] = useState({ active: false, elapsedMs: 0, paused: false });
  const [scroll, setScroll] = useState<{
    active: boolean;
    frames: number;
    height: number;
    previewUrl: string | null;
  }>({ active: false, frames: 0, height: 0, previewUrl: null });
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [captureError, setCaptureError] = useState<string | null>(null);
  const [screenGranted, setScreenGranted] = useState<boolean | null>(null);
  const pollPermission = useRef<ReturnType<typeof window.setInterval> | null>(null);
  // The tab showing before the editor opened; "Copy and delete" returns to it.
  const currentView = useRef<View>('capture');
  const viewBeforeEditor = useRef<View>('capture');

  const toast = useCallback((text: string, kind: 'ok' | 'err' = 'ok', ms = 4200) => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, text, kind }]);
    window.setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), ms);
  }, []);

  const refreshDisplays = useCallback(() => {
    api.listDisplays()
      .then((d) => {
        setDisplays(d);
        const primary = d.find((x) => x.isPrimary) ?? d[0];
        setDisplayId(primary ? primary.id : null);
      })
      .catch(() => {});
  }, []);

  const checkPermission = useCallback(
    (firePrompt: boolean) => {
      api.screenPermission()
        .then(async (granted) => {
          setScreenGranted(granted);
          if (!granted && firePrompt) await api.requestScreenPermission().catch(() => {});
          if (granted) {
            if (pollPermission.current) {
              window.clearInterval(pollPermission.current);
              pollPermission.current = null;
            }
            refreshDisplays();
            toast('Screen Recording permission granted');
          }
        })
        .catch(() => {});
    },
    [refreshDisplays, toast],
  );

  const refreshLibrary = useCallback(() => {
    api.listLibrary()
      .then(setItems)
      .catch(() => {});
  }, []);

  useEffect(() => {
    currentView.current = view;
  }, [view]);

  const openEditor = useCallback((item: CaptureItem) => {
    if (item.kind !== 'image') return;
    // Already in the editor: another capture replaced the one being edited,
    // so the tab to return to stays the same.
    if (currentView.current !== 'editor') viewBeforeEditor.current = currentView.current;
    setEditorItem(item);
    setView('editor');
  }, []);

  useEffect(() => {
    refreshLibrary();
    refreshDisplays();
    api.getSettings()
      .then(setSettings)
      .catch(() => {});

    // Ask macOS for Screen Recording on first launch; poll until granted.
    checkPermission(true);
    pollPermission.current = window.setInterval(() => checkPermission(false), 2500);

    const inactiveScroll = { active: false, frames: 0, height: 0, previewUrl: null };
    const unsubs: Promise<() => void>[] = [
      onEvent('library:changed', () => refreshLibrary()),
      onEvent<CaptureItem>('capture:saved', (item) => {
        setCaptureError(null);
        if (item.kind === 'image') openEditor(item);
        else toast('Capture saved to library');
      }),
      // Raised by the backend whenever a capture fails: the overlay is usually
      // already gone by then, so the main window has to be the one to say it.
      onEvent<{ message: string }>('capture:error', (p) => {
        setCaptureError(p.message);
        toast(p.message, 'err', 10000);
      }),
      onEvent('recording:started', () => setRec({ active: true, elapsedMs: 0, paused: false })),
      onEvent<{ elapsedMs: number }>('recording:tick', (p) =>
        setRec((r) => (r.active ? { ...r, elapsedMs: p.elapsedMs } : r)),
      ),
      onEvent<{ paused: boolean }>('recording:paused', (p) =>
        setRec((r) => ({ ...r, paused: p.paused })),
      ),
      onEvent<CaptureItem>('recording:done', () => {
        setRec({ active: false, elapsedMs: 0, paused: false });
        refreshLibrary();
        toast('Recording saved to library');
      }),
      onEvent<{ message: string }>('recording:error', (p) => {
        setRec({ active: false, elapsedMs: 0, paused: false });
        toast(p.message, 'err');
      }),
      onEvent<{ previewPath: string }>('scroll:started', (p) =>
        setScroll({ active: true, frames: 0, height: 0, previewUrl: convertFileSrc(p.previewPath) }),
      ),
      onEvent<{ frames: number; stitchedHeight: number }>('scroll:progress', (p) =>
        setScroll((s) => ({ ...s, frames: p.frames, height: p.stitchedHeight })),
      ),
      onEvent<CaptureItem>('scroll:done', (item) => {
        setScroll(inactiveScroll);
        refreshLibrary();
        if (item.kind === 'image') openEditor(item);
        else toast('Scrolling capture saved');
      }),
      onEvent<{ message: string }>('scroll:error', (p) => {
        setScroll(inactiveScroll);
        toast(p.message, 'err');
      }),
      onEvent<{ ready: boolean }>('ffmpeg:status', (p) => setFfmpegReady(p.ready)),
      onEvent('export:done', () => {
        refreshLibrary();
        toast('GIF export finished');
      }),
      onEvent<{ message: string }>('export:error', (p) => toast(p.message, 'err')),
    ];
    return () => {
      unsubs.forEach((u) =>
        u
          .then((f) => f())
          .catch(() => {}),
      );
      if (pollPermission.current) {
        window.clearInterval(pollPermission.current);
        pollPermission.current = null;
      }
    };
  }, [refreshLibrary, refreshDisplays, checkPermission, toast, openEditor]);

  return (
    <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
      <header className="flex items-center justify-between border-b border-zinc-800 px-4 py-3 sm:px-6">
        <div className="flex items-center gap-3">
          <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-400">
            <Aperture className="h-5 w-5" aria-hidden="true" />
          </div>
          <div>
            <div className="text-sm font-semibold tracking-wide">SnapPress</div>
            <div className="text-[11px] text-zinc-500">Rust capture studio</div>
          </div>
        </div>
        <nav className="flex items-center gap-1" aria-label="Main navigation">
          <NavButton icon={Scan} label="Capture" active={view === 'capture'} onClick={() => setView('capture')} />
          <NavButton icon={Images} label="Library" active={view === 'library'} onClick={() => setView('library')} />
          <button
            onClick={() => setSettingsOpen(true)}
            aria-label="Settings"
            title="Settings"
            className="rounded-lg p-2 text-zinc-400 transition hover:bg-zinc-900 hover:text-zinc-200"
          >
            <Settings className="h-4 w-4" aria-hidden="true" />
          </button>
        </nav>
      </header>

      {screenGranted === false && (
        <div className="flex flex-wrap items-center justify-between gap-2 border-b border-amber-500/40 bg-amber-500/10 px-4 py-2 text-sm text-amber-300 sm:px-6">
          <span>Screen Recording permission is required to capture your screen.</span>
          <span className="flex gap-2">
            <button
              onClick={() => api.openScreenSettings().catch(() => {})}
              className="rounded-lg border border-amber-500/50 px-2.5 py-1 transition hover:bg-amber-500/20"
            >
              Open System Settings
            </button>
            <button
              onClick={() => checkPermission(false)}
              className="rounded-lg px-2.5 py-1 text-amber-200/80 transition hover:text-amber-100"
            >
              Retry
            </button>
          </span>
        </div>
      )}

      <main className="min-h-0 flex-1 overflow-y-auto">
        {view === 'capture' && (
          <CaptureView
            displays={displays}
            displayId={displayId}
            onSelectDisplay={setDisplayId}
            items={items}
            ffmpegReady={ffmpegReady}
            onOpenEditor={openEditor}
            captureError={captureError}
            onDismissCaptureError={() => setCaptureError(null)}
          />
        )}
        {view === 'library' && <LibraryView items={items} onOpenEditor={openEditor} toast={toast} />}
        {view === 'editor' && editorItem && (
          <EditorView
            item={editorItem}
            onClose={() => setView('library')}
            onCopiedAndDeleted={() => {
              // Drop the capture from the list at once. The change event
              // arrives a moment later, and until then the tab we return to
              // would ask for a file that no longer exists.
              setItems((current) => current.filter((x) => x.id !== editorItem.id));
              setView(viewBeforeEditor.current);
              toast('Copied to clipboard and removed from the library');
            }}
          />
        )}
      </main>

      {rec.active && <RecorderBar elapsedMs={rec.elapsedMs} paused={rec.paused} />}
      {scroll.active && (
        <ScrollProgressCard frames={scroll.frames} height={scroll.height} previewUrl={scroll.previewUrl} />
      )}

      {settings && (
        <SettingsDialog
          open={settingsOpen}
          settings={settings}
          onClose={() => setSettingsOpen(false)}
          onSaved={(saved) => {
            setSettings(saved.settings);
            toast('Settings saved');
            saved.hotkeyWarnings.forEach((w) => toast(w, 'err', 9000));
          }}
        />
      )}

      <div className="pointer-events-none fixed bottom-4 right-4 z-[60] flex flex-col gap-2">
        {toasts.map((t) => (
          <div
            key={t.id}
            role="status"
            className={`rounded-lg border px-3 py-2 text-sm shadow-lg backdrop-blur ${
              t.kind === 'err'
                ? 'border-red-500/40 bg-red-500/10 text-red-300'
                : 'border-emerald-500/40 bg-emerald-500/10 text-emerald-300'
            }`}
          >
            {t.text}
          </div>
        ))}
      </div>
    </div>
  );
}

function NavButton({
  icon: Icon,
  label,
  active,
  onClick,
}: {
  icon: typeof Scan;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      aria-current={active ? 'page' : undefined}
      className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm transition ${
        active ? 'bg-emerald-500/15 text-emerald-400' : 'text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200'
      }`}
    >
      <Icon className="h-4 w-4" aria-hidden="true" /> {label}
    </button>
  );
}
