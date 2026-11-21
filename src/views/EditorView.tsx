import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';
import {
  ArrowUpRight,
  Circle,
  CircleDot,
  Copy,
  Crop,
  Droplets,
  Hash,
  Highlighter,
  MousePointer2,
  Pen,
  Redo2,
  Save,
  Square,
  Type,
  Undo2,
  type LucideIcon,
} from 'lucide-react';
import { api } from '../lib/api';
import type { CaptureItem } from '../lib/types';
import {
  buildExportCanvas,
  cloneLayer,
  createLayerId,
  handleAt,
  isLayerOutside,
  layerBBox,
  layerChanged,
  normalizeBox,
  renderScene,
  shiftLayer,
  topmostLayerAt,
} from './editor/render';
import type { EditorBox, EditorDoc, EditorLayer, EditorPoint, LayerKind } from './editor/render';

type ToolId =
  | 'select'
  | 'rect'
  | 'ellipse'
  | 'arrow'
  | 'pen'
  | 'text'
  | 'highlight'
  | 'blur'
  | 'pixelate'
  | 'number'
  | 'crop';

type DrawTool = 'rect' | 'ellipse' | 'arrow' | 'pen' | 'highlight' | 'blur' | 'pixelate';

type LoadState =
  | { status: 'loading' }
  | { status: 'error'; message: string }
  | { status: 'ready' };

type Interaction =
  | { mode: 'draw'; start: EditorPoint }
  | { mode: 'move'; id: string; start: EditorPoint; orig: EditorLayer }
  | { mode: 'resize-box'; id: string; handle: 0 | 1 | 2 | 3; orig: EditorLayer }
  | { mode: 'resize-arrow'; id: string; orig: EditorLayer }
  | { mode: 'crop'; start: EditorPoint };

interface ToolDef {
  id: ToolId;
  label: string;
  icon: LucideIcon;
}

const TOOLS: ToolDef[] = [
  { id: 'select', label: 'Select and move', icon: MousePointer2 },
  { id: 'rect', label: 'Rectangle', icon: Square },
  { id: 'ellipse', label: 'Ellipse', icon: Circle },
  { id: 'arrow', label: 'Arrow', icon: ArrowUpRight },
  { id: 'pen', label: 'Freehand pen', icon: Pen },
  { id: 'text', label: 'Text', icon: Type },
  { id: 'highlight', label: 'Highlighter', icon: Highlighter },
  { id: 'blur', label: 'Blur region', icon: Droplets },
  { id: 'pixelate', label: 'Pixelate region', icon: Hash },
  { id: 'number', label: 'Number badge', icon: CircleDot },
  { id: 'crop', label: 'Crop', icon: Crop },
];

const DRAW_TOOLS: ToolId[] = ['rect', 'ellipse', 'arrow', 'pen', 'highlight', 'blur', 'pixelate'];
const RECT_LIKE: LayerKind[] = ['rect', 'ellipse', 'highlight', 'blur', 'pixelate'];

const COLORS = ['#fafafa', '#18181b', '#10b981', '#ef4444', '#f59e0b', '#ec4899'];
const STROKE_WIDTHS = [2, 4, 8];
const HISTORY_LIMIT = 30;
/** Matches the p-6 padding of the canvas stage (both sides). */
const STAGE_PADDING = 24;

const HEADER_ICON_BUTTON =
  'inline-flex h-8 min-w-[32px] items-center justify-center gap-1.5 rounded px-2 text-xs font-medium text-zinc-300 transition-colors hover:bg-zinc-800 hover:text-zinc-100 disabled:pointer-events-none disabled:opacity-40';
const PRIMARY_BUTTON =
  'inline-flex h-8 items-center justify-center rounded bg-emerald-500 px-3 text-xs font-semibold text-zinc-950 transition-colors hover:bg-emerald-400 disabled:pointer-events-none disabled:opacity-40';

function isDrawTool(tool: ToolId): tool is DrawTool {
  return (
    tool === 'rect' ||
    tool === 'ellipse' ||
    tool === 'arrow' ||
    tool === 'pen' ||
    tool === 'highlight' ||
    tool === 'blur' ||
    tool === 'pixelate'
  );
}

function isDrawableShape(layer: EditorLayer): boolean {
  switch (layer.kind) {
    case 'pen':
      return (layer.points?.length ?? 0) >= 2;
    case 'arrow':
      return Math.hypot((layer.x2 ?? layer.x) - layer.x, (layer.y2 ?? layer.y) - layer.y) >= 2;
    case 'rect':
    case 'ellipse':
    case 'highlight':
    case 'blur':
    case 'pixelate':
      return layer.w >= 1 && layer.h >= 1;
    default:
      return false;
  }
}

function snapshotOf(doc: EditorDoc): EditorDoc {
  return {
    layers: doc.layers.map((l) => cloneLayer(l)),
    baseImage: doc.baseImage,
    baseW: doc.baseW,
    baseH: doc.baseH,
    cropX: doc.cropX,
    cropY: doc.cropY,
  };
}

function clampPoint(p: EditorPoint, doc: EditorDoc): EditorPoint {
  return {
    x: Math.min(Math.max(0, p.x), doc.baseW),
    y: Math.min(Math.max(0, p.y), doc.baseH),
  };
}

function fileNameOf(path: string): string {
  const idx = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return idx >= 0 ? path.slice(idx + 1) : path;
}

/**
 * Canvas annotation editor. The visible canvas is the image's natural pixel
 * size, scaled down via CSS; all layer coordinates are stored in image pixel
 * space and converted from pointer coordinates through the display scale.
 */
export default function EditorView(props: { item: CaptureItem; onClose: () => void }): JSX.Element {
  const { item, onClose } = props;

  const [load, setLoad] = useState<LoadState>({ status: 'loading' });
  const [doc, setDoc] = useState<EditorDoc | null>(null);
  const [tool, setTool] = useState<ToolId>('select');
  const [color, setColor] = useState<string>('#10b981');
  const [strokeWidth, setStrokeWidth] = useState<number>(4);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [cropRect, setCropRect] = useState<EditorBox | null>(null);
  const [textDraft, setTextDraft] = useState<EditorPoint | null>(null);
  const [stageSize, setStageSize] = useState<{ w: number; h: number }>({ w: 960, h: 640 });
  const [stacks, setStacks] = useState<{ undo: number; redo: number }>({ undo: 0, redo: 0 });
  const [status, setStatus] = useState<{ text: string; ok: boolean } | null>(null);
  const [busy, setBusy] = useState(false);

  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const stageRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const docRef = useRef<EditorDoc | null>(null);
  const selectedIdRef = useRef<string | null>(null);
  const draftRef = useRef<EditorLayer | null>(null);
  const interactionRef = useRef<Interaction | null>(null);
  const cropDraftRef = useRef<EditorBox | null>(null);
  const textDraftRef = useRef<{ x: number; y: number; done: boolean } | null>(null);
  const undoStackRef = useRef<EditorDoc[]>([]);
  const redoStackRef = useRef<EditorDoc[]>([]);
  const drawRef = useRef<() => void>(() => undefined);
  const rafRef = useRef<number | null>(null);
  const statusTimerRef = useRef<number | null>(null);

  const maxW = Math.max(160, stageSize.w - STAGE_PADDING * 2);
  const maxH = Math.max(160, stageSize.h - STAGE_PADDING * 2);
  const scale = doc ? Math.min(maxW / doc.baseW, maxH / doc.baseH, 1) : 1;
  const displayW = doc ? doc.baseW * scale : 0;
  const displayH = doc ? doc.baseH * scale : 0;

  const fileName = useMemo(() => fileNameOf(item.path), [item.path]);

  // Keep the refs used by event handlers and the rAF draw loop in sync.
  useEffect(() => {
    docRef.current = doc;
  }, [doc]);
  useEffect(() => {
    selectedIdRef.current = selectedId;
  }, [selectedId]);

  // Track the available stage size so the canvas can be scaled to fit.
  useEffect(() => {
    const el = stageRef.current;
    if (!el) return undefined;
    const update = () => setStageSize({ w: el.clientWidth, h: el.clientHeight });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Load the base image through the base64 API (never the asset protocol:
  // protocol URLs would taint the canvas and break export).
  useEffect(() => {
    let cancelled = false;
    setLoad({ status: 'loading' });
    setDoc(null);
    docRef.current = null;
    setSelectedId(null);
    selectedIdRef.current = null;
    setCropRect(null);
    setTextDraft(null);
    textDraftRef.current = null;
    draftRef.current = null;
    interactionRef.current = null;
    cropDraftRef.current = null;
    undoStackRef.current = [];
    redoStackRef.current = [];
    setStacks({ undo: 0, redo: 0 });
    setStatus(null);
    api
      .readImageBase64(item.path)
      .then(
        (b64) =>
          new Promise<HTMLImageElement>((resolve, reject) => {
            const img = new Image();
            img.onload = () => resolve(img);
            img.onerror = () => reject(new Error('The captured image could not be decoded'));
            img.src = `data:image/png;base64,${b64}`;
          }),
      )
      .then((img) => {
        if (cancelled) return;
        const next: EditorDoc = {
          layers: [],
          baseImage: img,
          baseW: Math.max(1, img.naturalWidth),
          baseH: Math.max(1, img.naturalHeight),
          cropX: 0,
          cropY: 0,
        };
        docRef.current = next;
        setDoc(next);
        setLoad({ status: 'ready' });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setLoad({
          status: 'error',
          message: err instanceof Error ? err.message : 'Failed to load the image',
        });
      });
    return () => {
      cancelled = true;
    };
  }, [item.path]);

  // Central draw path: runs on every committed state change; drags re-invoke
  // drawRef.current directly through requestAnimationFrame.
  useEffect(() => {
    if (load.status !== 'ready' || !doc) return undefined;
    const canvas = canvasRef.current;
    if (!canvas) return undefined;
    const ctx = canvas.getContext('2d');
    if (!ctx) return undefined;
    const draw = () => {
      const current = docRef.current;
      if (!current) return;
      renderScene(ctx, {
        baseImage: current.baseImage,
        baseW: current.baseW,
        baseH: current.baseH,
        layers: current.layers,
        draft: draftRef.current,
        selectedId: selectedIdRef.current,
        cropRect: cropDraftRef.current ?? cropRect,
        scale,
      });
    };
    drawRef.current = draw;
    draw();
    return undefined;
  }, [doc, cropRect, scale, load.status, selectedId]);

  const requestDraw = useCallback(() => {
    if (rafRef.current !== null) return;
    rafRef.current = window.requestAnimationFrame(() => {
      rafRef.current = null;
      drawRef.current();
    });
  }, []);

  useEffect(
    () => () => {
      if (rafRef.current !== null) window.cancelAnimationFrame(rafRef.current);
      if (statusTimerRef.current !== null) window.clearTimeout(statusTimerRef.current);
    },
    [],
  );

  const pushHistory = useCallback(() => {
    const current = docRef.current;
    if (!current) return;
    const stack = undoStackRef.current;
    stack.push(snapshotOf(current));
    if (stack.length > HISTORY_LIMIT) stack.shift();
    redoStackRef.current = [];
    setStacks({ undo: stack.length, redo: 0 });
  }, []);

  const commitDoc = useCallback((next: EditorDoc) => {
    docRef.current = next;
    setDoc(next);
  }, []);

  const undo = useCallback(() => {
    const stack = undoStackRef.current;
    const current = docRef.current;
    if (stack.length === 0 || !current) return;
    const snap = stack.pop();
    if (!snap) return;
    redoStackRef.current.push(snapshotOf(current));
    draftRef.current = null;
    interactionRef.current = null;
    docRef.current = snap;
    setDoc(snap);
    setSelectedId(null);
    setStacks({ undo: stack.length, redo: redoStackRef.current.length });
  }, []);

  const redo = useCallback(() => {
    const stack = redoStackRef.current;
    const current = docRef.current;
    if (stack.length === 0 || !current) return;
    const snap = stack.pop();
    if (!snap) return;
    undoStackRef.current.push(snapshotOf(current));
    draftRef.current = null;
    interactionRef.current = null;
    docRef.current = snap;
    setDoc(snap);
    setSelectedId(null);
    setStacks({ undo: undoStackRef.current.length, redo: stack.length });
  }, []);

  const deleteSelected = useCallback(() => {
    const current = docRef.current;
    const id = selectedIdRef.current;
    if (!current || !id) return;
    if (!current.layers.some((l) => l.id === id)) return;
    pushHistory();
    commitDoc({ ...current, layers: current.layers.filter((l) => l.id !== id) });
    setSelectedId(null);
    selectedIdRef.current = null;
  }, [pushHistory, commitDoc]);

  // Keyboard shortcuts: undo/redo, delete selected layer, escape deselects.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const inInput =
        !!target &&
        (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable);
      const mod = e.ctrlKey || e.metaKey;
      if (mod && !e.altKey && e.key.toLowerCase() === 'z') {
        if (inInput) return;
        e.preventDefault();
        if (e.shiftKey) redo();
        else undo();
        return;
      }
      if ((e.key === 'Delete' || e.key === 'Backspace') && !inInput) {
        if (selectedIdRef.current) {
          e.preventDefault();
          deleteSelected();
        }
        return;
      }
      if (e.key === 'Escape' && !inInput) {
        interactionRef.current = null;
        draftRef.current = null;
        cropDraftRef.current = null;
        setSelectedId(null);
        selectedIdRef.current = null;
        requestDraw();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [undo, redo, deleteSelected, requestDraw]);

  const finishText = useCallback(
    (commit: boolean) => {
      const draft = textDraftRef.current;
      if (!draft || draft.done) return;
      draft.done = true;
      textDraftRef.current = null;
      setTextDraft(null);
      const value = (inputRef.current?.value ?? '').trim();
      const current = docRef.current;
      if (!commit || !value || !current) return;
      const layer: EditorLayer = {
        id: createLayerId(),
        kind: 'text',
        x: draft.x,
        y: draft.y,
        w: 0,
        h: 0,
        text: value,
        color,
        strokeWidth,
      };
      pushHistory();
      commitDoc({ ...current, layers: [...current.layers, layer] });
    },
    [color, strokeWidth, pushHistory, commitDoc],
  );

  const showStatus = useCallback((text: string, ok: boolean) => {
    if (statusTimerRef.current !== null) window.clearTimeout(statusTimerRef.current);
    setStatus({ text, ok });
    statusTimerRef.current = window.setTimeout(() => {
      statusTimerRef.current = null;
      setStatus(null);
    }, 2500);
  }, []);

  const handleCopy = async () => {
    const current = docRef.current;
    if (!current || busy) return;
    setBusy(true);
    try {
      const dataUrl = buildExportCanvas(current).toDataURL('image/png');
      const saved = await api.saveEdited(dataUrl, 'png');
      await api.copyImageToClipboard(saved.path);
      showStatus('Copied to clipboard', true);
    } catch (err) {
      showStatus(err instanceof Error ? err.message : 'Copy failed', false);
    } finally {
      setBusy(false);
    }
  };

  const handleExport = async (format: 'png' | 'jpeg') => {
    const current = docRef.current;
    if (!current || busy) return;
    setBusy(true);
    try {
      const dataUrl = buildExportCanvas(current).toDataURL(
        format === 'jpeg' ? 'image/jpeg' : 'image/png',
        0.92,
      );
      await api.saveEdited(dataUrl, format);
      showStatus(format === 'jpeg' ? 'Saved JPG to library' : 'Saved PNG to library', true);
    } catch (err) {
      showStatus(err instanceof Error ? err.message : 'Export failed', false);
    } finally {
      setBusy(false);
    }
  };

  const handleApplyCrop = () => {
    const current = docRef.current;
    const box = cropRect;
    if (!current || !box) return;
    const x = Math.max(0, Math.floor(box.x));
    const y = Math.max(0, Math.floor(box.y));
    const w = Math.min(current.baseW, Math.ceil(box.x + box.w)) - x;
    const h = Math.min(current.baseH, Math.ceil(box.y + box.h)) - y;
    if (w < 1 || h < 1) {
      setCropRect(null);
      requestDraw();
      return;
    }
    const cropped = document.createElement('canvas');
    cropped.width = w;
    cropped.height = h;
    const cctx = cropped.getContext('2d');
    if (!cctx) return;
    cctx.drawImage(current.baseImage, x, y, w, h, 0, 0, w, h);
    const layers = current.layers
      .map((l) => shiftLayer(l, -x, -y))
      .filter((l) => !isLayerOutside(l, w, h));
    pushHistory();
    const next: EditorDoc = {
      layers,
      baseImage: cropped,
      baseW: w,
      baseH: h,
      cropX: current.cropX + x,
      cropY: current.cropY + y,
    };
    draftRef.current = null;
    interactionRef.current = null;
    cropDraftRef.current = null;
    docRef.current = next;
    setDoc(next);
    setCropRect(null);
    setSelectedId(null);
    selectedIdRef.current = null;
    requestDraw();
  };

  const applyStyleToSelection = (patch: { color?: string; strokeWidth?: number }) => {
    const current = docRef.current;
    const id = selectedIdRef.current;
    if (!current || !id) return;
    const selected = current.layers.find((l) => l.id === id);
    if (!selected) return;
    const colorChanged = patch.color !== undefined && patch.color !== selected.color;
    const widthChanged = patch.strokeWidth !== undefined && patch.strokeWidth !== selected.strokeWidth;
    if (!colorChanged && !widthChanged) return;
    pushHistory();
    commitDoc({
      ...current,
      layers: current.layers.map((l) => (l.id === id ? { ...l, ...patch } : l)),
    });
  };

  const handleColorPick = (c: string) => {
    setColor(c);
    applyStyleToSelection({ color: c });
  };

  const handleWidthPick = (w: number) => {
    setStrokeWidth(w);
    applyStyleToSelection({ strokeWidth: w });
  };

  const imagePointFromEvent = (e: ReactPointerEvent<HTMLCanvasElement>): EditorPoint => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0 };
    const rect = canvas.getBoundingClientRect();
    return {
      x: (e.clientX - rect.left) / scale,
      y: (e.clientY - rect.top) / scale,
    };
  };

  const updateHoverCursor = (e: ReactPointerEvent<HTMLCanvasElement>, current: EditorDoc) => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const p = imagePointFromEvent(e);
    let cursor = 'default';
    const selected = selectedIdRef.current
      ? current.layers.find((l) => l.id === selectedIdRef.current)
      : undefined;
    if (selected) {
      const handle = handleAt(layerBBox(selected), p, 7 / scale);
      if (handle === 0 || handle === 2) cursor = 'nwse-resize';
      else if (handle === 1 || handle === 3) cursor = 'nesw-resize';
    }
    if (cursor === 'default' && topmostLayerAt(current.layers, p)) cursor = 'move';
    canvas.style.cursor = cursor;
  };

  const onPointerDown = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const current = docRef.current;
    const canvas = canvasRef.current;
    if (!current || !canvas || load.status !== 'ready') return;
    if (e.button !== 0) return;
    e.preventDefault();
    // A click anywhere on the canvas first commits a pending text placement.
    if (textDraftRef.current) {
      finishText(true);
      return;
    }
    const p = clampPoint(imagePointFromEvent(e), current);
    try {
      canvas.setPointerCapture(e.pointerId);
    } catch {
      // Pointer capture is best-effort; dragging still works inside the canvas.
    }

    if (tool === 'select') {
      const selected = selectedIdRef.current
        ? current.layers.find((l) => l.id === selectedIdRef.current)
        : undefined;
      if (selected) {
        const box = layerBBox(selected);
        const handle = handleAt(box, p, 7 / scale);
        if (handle !== null) {
          if (RECT_LIKE.includes(selected.kind)) {
            interactionRef.current = {
              mode: 'resize-box',
              id: selected.id,
              handle,
              orig: cloneLayer(selected),
            };
          } else if (selected.kind === 'arrow') {
            interactionRef.current = {
              mode: 'resize-arrow',
              id: selected.id,
              orig: cloneLayer(selected),
            };
          } else {
            // pen / text / number layers can only be moved.
            interactionRef.current = { mode: 'move', id: selected.id, start: p, orig: cloneLayer(selected) };
          }
          draftRef.current = cloneLayer(selected);
          requestDraw();
          return;
        }
      }
      const hit = topmostLayerAt(current.layers, p);
      if (hit) {
        setSelectedId(hit.id);
        selectedIdRef.current = hit.id;
        interactionRef.current = { mode: 'move', id: hit.id, start: p, orig: cloneLayer(hit) };
        draftRef.current = cloneLayer(hit);
      } else {
        setSelectedId(null);
        selectedIdRef.current = null;
      }
      requestDraw();
      return;
    }

    if (tool === 'number') {
      const badge = current.layers.filter((l) => l.kind === 'number').length + 1;
      const layer: EditorLayer = {
        id: createLayerId(),
        kind: 'number',
        x: p.x,
        y: p.y,
        w: 0,
        h: 0,
        badge,
        color,
        strokeWidth,
      };
      pushHistory();
      commitDoc({ ...current, layers: [...current.layers, layer] });
      return;
    }

    if (tool === 'text') {
      textDraftRef.current = { x: p.x, y: p.y, done: false };
      setTextDraft({ x: p.x, y: p.y });
      return;
    }

    if (tool === 'crop') {
      interactionRef.current = { mode: 'crop', start: p };
      cropDraftRef.current = { x: p.x, y: p.y, w: 0, h: 0 };
      requestDraw();
      return;
    }

    if (!isDrawTool(tool)) return;
    const layer: EditorLayer = {
      id: createLayerId(),
      kind: tool,
      x: p.x,
      y: p.y,
      w: 0,
      h: 0,
      color,
      strokeWidth,
    };
    if (tool === 'arrow') {
      layer.x2 = p.x;
      layer.y2 = p.y;
    }
    if (tool === 'pen') {
      layer.points = [{ x: p.x, y: p.y }];
    }
    interactionRef.current = { mode: 'draw', start: p };
    draftRef.current = layer;
    requestDraw();
  };

  const onPointerMove = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const current = docRef.current;
    if (!current) return;
    const interaction = interactionRef.current;
    if (!interaction) {
      if (tool === 'select' && load.status === 'ready') updateHoverCursor(e, current);
      return;
    }
    const p = clampPoint(imagePointFromEvent(e), current);
    const draft = draftRef.current;

    if (interaction.mode === 'draw' && draft) {
      if (draft.kind === 'pen') {
        const pts = draft.points;
        if (pts) {
          const last = pts[pts.length - 1];
          if (!last || Math.hypot(p.x - last.x, p.y - last.y) >= 2) pts.push({ x: p.x, y: p.y });
        }
      } else if (draft.kind === 'arrow') {
        draft.x2 = p.x;
        draft.y2 = p.y;
      } else {
        const box = normalizeBox(
          interaction.start.x,
          interaction.start.y,
          p.x - interaction.start.x,
          p.y - interaction.start.y,
        );
        draft.x = box.x;
        draft.y = box.y;
        draft.w = box.w;
        draft.h = box.h;
      }
      requestDraw();
      return;
    }

    if (interaction.mode === 'move' && draft) {
      const dx = p.x - interaction.start.x;
      const dy = p.y - interaction.start.y;
      draft.x = interaction.orig.x + dx;
      draft.y = interaction.orig.y + dy;
      if (interaction.orig.x2 !== undefined) draft.x2 = interaction.orig.x2 + dx;
      if (interaction.orig.y2 !== undefined) draft.y2 = interaction.orig.y2 + dy;
      if (interaction.orig.points) {
        draft.points = interaction.orig.points.map((pt) => ({ x: pt.x + dx, y: pt.y + dy }));
      }
      requestDraw();
      return;
    }

    if (interaction.mode === 'resize-box' && draft) {
      const base = layerBBox(interaction.orig);
      // Opposite corner stays fixed while the grabbed corner follows the pointer.
      const fixed =
        interaction.handle === 0
          ? { x: base.x + base.w, y: base.y + base.h }
          : interaction.handle === 1
            ? { x: base.x, y: base.y + base.h }
            : interaction.handle === 2
              ? { x: base.x, y: base.y }
              : { x: base.x + base.w, y: base.y };
      const box = normalizeBox(fixed.x, fixed.y, p.x - fixed.x, p.y - fixed.y);
      draft.x = box.x;
      draft.y = box.y;
      draft.w = box.w;
      draft.h = box.h;
      requestDraw();
      return;
    }

    if (interaction.mode === 'resize-arrow' && draft) {
      draft.x2 = p.x;
      draft.y2 = p.y;
      requestDraw();
      return;
    }

    if (interaction.mode === 'crop') {
      cropDraftRef.current = normalizeBox(
        interaction.start.x,
        interaction.start.y,
        p.x - interaction.start.x,
        p.y - interaction.start.y,
      );
      requestDraw();
    }
  };

  const onPointerUp = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const current = docRef.current;
    const canvas = canvasRef.current;
    if (canvas) {
      try {
        canvas.releasePointerCapture(e.pointerId);
      } catch {
        // Ignore: the pointer may already have been released.
      }
    }
    const interaction = interactionRef.current;
    if (!current || !interaction) return;
    interactionRef.current = null;

    if (interaction.mode === 'draw') {
      const draft = draftRef.current;
      draftRef.current = null;
      if (draft && isDrawableShape(draft)) {
        pushHistory();
        commitDoc({ ...current, layers: [...current.layers, cloneLayer(draft)] });
      }
      requestDraw();
      return;
    }

    if (interaction.mode === 'crop') {
      const box = cropDraftRef.current;
      cropDraftRef.current = null;
      setCropRect(box && box.w >= 3 && box.h >= 3 ? box : null);
      requestDraw();
      return;
    }

    // move / resize commits happen on pointer-up and push a history snapshot.
    const draft = draftRef.current;
    draftRef.current = null;
    if (draft && layerChanged(draft, interaction.orig)) {
      pushHistory();
      commitDoc({
        ...current,
        layers: current.layers.map((l) => (l.id === draft.id ? cloneLayer(draft) : l)),
      });
    }
    requestDraw();
  };

  // Reset the cursor when switching tools.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas) canvas.style.cursor = tool === 'select' ? 'default' : 'crosshair';
  }, [tool, load.status]);

  return (
    <div className="flex h-full flex-col bg-zinc-950 text-zinc-200">
      <div className="flex items-center gap-2 border-b border-zinc-800 px-4 py-2">
        <div className="min-w-0">
          <div className="truncate text-sm font-medium text-zinc-100" title={item.path}>
            {fileName}
          </div>
          <div className="text-xs text-zinc-500">
            {doc ? `${doc.baseW} x ${doc.baseH}` : `${item.width} x ${item.height}`}
          </div>
        </div>
        <div className="flex-1" />
        {status && (
          <span
            role="status"
            className={`text-xs ${status.ok ? 'text-emerald-400' : 'text-red-400'}`}
          >
            {status.text}
          </span>
        )}
        <button
          type="button"
          title="Undo (Ctrl+Z)"
          aria-label="Undo"
          disabled={stacks.undo === 0}
          onClick={() => undo()}
          className={HEADER_ICON_BUTTON}
        >
          <Undo2 size={16} />
        </button>
        <button
          type="button"
          title="Redo (Ctrl+Shift+Z)"
          aria-label="Redo"
          disabled={stacks.redo === 0}
          onClick={() => redo()}
          className={HEADER_ICON_BUTTON}
        >
          <Redo2 size={16} />
        </button>
        {cropRect && (
          <button
            type="button"
            title="Apply crop"
            aria-label="Apply crop"
            onClick={handleApplyCrop}
            className={PRIMARY_BUTTON}
          >
            Apply crop
          </button>
        )}
        <div className="h-5 w-px bg-zinc-800" />
        <button
          type="button"
          title="Copy image to clipboard"
          aria-label="Copy image to clipboard"
          disabled={busy}
          onClick={handleCopy}
          className={HEADER_ICON_BUTTON}
        >
          <Copy size={16} />
        </button>
        <button
          type="button"
          title="Save as PNG to library"
          aria-label="Save as PNG"
          disabled={busy}
          onClick={() => handleExport('png')}
          className={HEADER_ICON_BUTTON}
        >
          <Save size={16} />
          <span>PNG</span>
        </button>
        <button
          type="button"
          title="Save as JPG to library"
          aria-label="Save as JPG"
          disabled={busy}
          onClick={() => handleExport('jpeg')}
          className={HEADER_ICON_BUTTON}
        >
          <Save size={16} />
          <span>JPG</span>
        </button>
        <div className="h-5 w-px bg-zinc-800" />
        <button type="button" title="Done" aria-label="Done" onClick={onClose} className={PRIMARY_BUTTON}>
          Done
        </button>
      </div>

      <div className="flex min-h-0 flex-1">
        <div className="flex w-12 flex-col items-center gap-1 overflow-y-auto border-r border-zinc-800 py-3">
          {TOOLS.map((t) => (
            <button
              key={t.id}
              type="button"
              title={t.label}
              aria-label={t.label}
              aria-pressed={tool === t.id}
              onClick={() => setTool(t.id)}
              className={`flex h-9 w-9 shrink-0 items-center justify-center rounded transition-colors ${
                tool === t.id
                  ? 'bg-emerald-500/15 text-emerald-400'
                  : 'text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100'
              }`}
            >
              <t.icon size={18} />
            </button>
          ))}
        </div>
        <div
          ref={stageRef}
          className="flex flex-1 items-center justify-center overflow-auto bg-zinc-950 p-6"
        >
          {load.status === 'loading' && (
            <div
              role="status"
              aria-label="Loading image"
              className="h-8 w-8 shrink-0 animate-spin rounded-full border-2 border-zinc-700 border-t-emerald-400"
            />
          )}
          {load.status === 'error' && (
            <div className="flex shrink-0 flex-col items-center gap-3 text-center">
              <p className="text-sm text-red-400">{load.message}</p>
              <button type="button" onClick={onClose} className={PRIMARY_BUTTON}>
                Back to library
              </button>
            </div>
          )}
          {load.status === 'ready' && doc && (
            <div className="relative m-auto" style={{ width: displayW, height: displayH }}>
              <canvas
                ref={canvasRef}
                width={doc.baseW}
                height={doc.baseH}
                className="block touch-none select-none"
                style={{ width: displayW, height: displayH }}
                onPointerDown={onPointerDown}
                onPointerMove={onPointerMove}
                onPointerUp={onPointerUp}
                onPointerCancel={onPointerUp}
              />
              {textDraft && (
                <input
                  ref={inputRef}
                  type="text"
                  defaultValue=""
                  autoFocus
                  aria-label="Annotation text"
                  placeholder="Type text, press Enter"
                  className="absolute z-10 w-[200px] rounded border border-emerald-500/60 bg-zinc-900 px-2 py-1 text-sm text-zinc-100 outline-none"
                  style={{ left: textDraft.x * scale, top: textDraft.y * scale }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') finishText(true);
                    else if (e.key === 'Escape') finishText(false);
                  }}
                  onBlur={() => finishText(true)}
                />
              )}
            </div>
          )}
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-3 border-t border-zinc-800 px-4 py-2">
        <span className="text-xs uppercase tracking-wide text-zinc-500">Color</span>
        <div className="flex items-center gap-1">
          {COLORS.map((c) => (
            <button
              key={c}
              type="button"
              title={`Color ${c}`}
              aria-label={`Color ${c}`}
              aria-pressed={color === c}
              onClick={() => handleColorPick(c)}
              className="flex h-8 w-8 items-center justify-center rounded transition-colors hover:bg-zinc-800"
            >
              <span
                className={`block h-5 w-5 rounded-full border border-zinc-600 ${
                  color === c ? 'ring-2 ring-emerald-400' : ''
                }`}
                style={{ backgroundColor: c }}
              />
            </button>
          ))}
        </div>
        <div className="h-5 w-px bg-zinc-800" />
        <span className="text-xs uppercase tracking-wide text-zinc-500">Stroke</span>
        <div className="flex items-center gap-1">
          {STROKE_WIDTHS.map((w) => (
            <button
              key={w}
              type="button"
              title={`Stroke width ${w}`}
              aria-label={`Stroke width ${w}`}
              aria-pressed={strokeWidth === w}
              onClick={() => handleWidthPick(w)}
              className={`flex h-8 w-8 items-center justify-center rounded transition-colors ${
                strokeWidth === w
                  ? 'bg-emerald-500/15 text-emerald-400'
                  : 'text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100'
              }`}
            >
              <span className="block rounded-full bg-current" style={{ width: 18, height: w }} />
            </button>
          ))}
        </div>
        <div className="flex-1" />
        <span className="text-xs text-zinc-600">
          {tool === 'select'
            ? 'Click a layer to select it, drag to move'
            : TOOLS.find((t) => t.id === tool)?.label ?? ''}
        </span>
      </div>
    </div>
  );
}
