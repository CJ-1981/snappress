/*
 * Pure canvas rendering helpers for the annotation editor.
 *
 * This module is framework-free: it only touches DOM canvas APIs so the same
 * code can be used for the interactive editor view and for offscreen export
 * rendering. All layer coordinates live in IMAGE pixel space (the natural
 * pixel size of the base screenshot).
 */

export type LayerKind =
  | 'rect'
  | 'ellipse'
  | 'arrow'
  | 'pen'
  | 'text'
  | 'highlight'
  | 'blur'
  | 'pixelate'
  | 'number';

export interface EditorPoint {
  x: number;
  y: number;
}

export interface EditorBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface EditorLayer {
  id: string;
  kind: LayerKind;
  /** Anchor point / rect top-left in image pixel space. */
  x: number;
  y: number;
  /** Rect size (rect, ellipse, highlight, blur, pixelate). */
  w: number;
  h: number;
  /** Arrow end point in image pixel space. */
  x2?: number;
  y2?: number;
  /** Pen polyline points in image pixel space. */
  points?: EditorPoint[];
  /** Text content for text layers. */
  text?: string;
  /** Badge index for number layers. */
  badge?: number;
  color: string;
  strokeWidth: number;
}

export interface EditorDoc {
  layers: EditorLayer[];
  baseImage: HTMLImageElement | HTMLCanvasElement;
  baseW: number;
  baseH: number;
  /** Cumulative offset of the current base image inside the original capture. */
  cropX: number;
  cropY: number;
}

export interface SceneState {
  baseImage: HTMLImageElement | HTMLCanvasElement;
  baseW: number;
  baseH: number;
  layers: EditorLayer[];
  /** In-progress layer (new shape, or a copy of the layer being moved/resized). */
  draft: EditorLayer | null;
  selectedId: string | null;
  cropRect: EditorBox | null;
  /** CSS display scale (canvas pixels / css pixels), used to keep UI overlays constant on screen. */
  scale: number;
}

const NUMBER_RADIUS = 14;
const PIXELATE_BLOCK = 12;
const BLUR_DOWNSCALE = 0.1;
const FONT_STACK = 'ui-sans-serif, system-ui, sans-serif';
const SELECTION_COLOR = '#34d399';

let layerSeq = 0;

export function createLayerId(): string {
  layerSeq += 1;
  return `layer-${layerSeq}`;
}

export function normalizeBox(x: number, y: number, w: number, h: number): EditorBox {
  return {
    x: w < 0 ? x + w : x,
    y: h < 0 ? y + h : y,
    w: Math.abs(w),
    h: Math.abs(h),
  };
}

export function cloneLayer(layer: EditorLayer): EditorLayer {
  return {
    ...layer,
    points: layer.points ? layer.points.map((p) => ({ x: p.x, y: p.y })) : undefined,
  };
}

export function textFontSize(layer: EditorLayer): number {
  return Math.max(16, layer.strokeWidth * 6);
}

let measureCtx: CanvasRenderingContext2D | null = null;

function getMeasureCtx(): CanvasRenderingContext2D {
  if (!measureCtx) {
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('Canvas 2D context is unavailable');
    measureCtx = ctx;
  }
  return measureCtx;
}

/** Bounding box of a layer in image pixel space (used for hit tests and selection UI). */
export function layerBBox(layer: EditorLayer): EditorBox {
  switch (layer.kind) {
    case 'rect':
    case 'ellipse':
    case 'highlight':
    case 'blur':
    case 'pixelate':
      return normalizeBox(layer.x, layer.y, layer.w, layer.h);
    case 'arrow': {
      const x2 = layer.x2 ?? layer.x;
      const y2 = layer.y2 ?? layer.y;
      const pad = Math.max(layer.strokeWidth, 1);
      return normalizeBox(
        Math.min(layer.x, x2) - pad,
        Math.min(layer.y, y2) - pad,
        Math.abs(x2 - layer.x) + pad * 2,
        Math.abs(y2 - layer.y) + pad * 2,
      );
    }
    case 'pen': {
      const pts = layer.points && layer.points.length > 0 ? layer.points : [{ x: layer.x, y: layer.y }];
      const first = pts[0];
      if (!first) return normalizeBox(layer.x, layer.y, 1, 1);
      let minX = first.x;
      let maxX = first.x;
      let minY = first.y;
      let maxY = first.y;
      for (const p of pts) {
        if (p.x < minX) minX = p.x;
        if (p.x > maxX) maxX = p.x;
        if (p.y < minY) minY = p.y;
        if (p.y > maxY) maxY = p.y;
      }
      const pad = Math.max(layer.strokeWidth, 1);
      return { x: minX - pad, y: minY - pad, w: maxX - minX + pad * 2, h: maxY - minY + pad * 2 };
    }
    case 'text': {
      const size = textFontSize(layer);
      const ctx = getMeasureCtx();
      ctx.font = `${size}px ${FONT_STACK}`;
      const width = ctx.measureText(layer.text ?? '').width;
      return { x: layer.x, y: layer.y, w: Math.max(width, 8), h: size * 1.3 };
    }
    case 'number':
      return {
        x: layer.x - NUMBER_RADIUS,
        y: layer.y - NUMBER_RADIUS,
        w: NUMBER_RADIUS * 2,
        h: NUMBER_RADIUS * 2,
      };
    default:
      return normalizeBox(layer.x, layer.y, Math.max(layer.w, 1), Math.max(layer.h, 1));
  }
}

/** Topmost layer (later in the array = on top) whose bbox contains the point. */
export function topmostLayerAt(layers: EditorLayer[], p: EditorPoint): EditorLayer | null {
  for (let i = layers.length - 1; i >= 0; i -= 1) {
    const layer = layers[i];
    if (!layer) continue;
    const box = layerBBox(layer);
    if (p.x >= box.x && p.x <= box.x + box.w && p.y >= box.y && p.y <= box.y + box.h) {
      return layer;
    }
  }
  return null;
}

/**
 * Which of the 4 corner handles (0=top-left, 1=top-right, 2=bottom-right,
 * 3=bottom-left) is within tolerance of the point, if any.
 */
export function handleAt(box: EditorBox, p: EditorPoint, tolerance: number): 0 | 1 | 2 | 3 | null {
  const corners: EditorPoint[] = [
    { x: box.x, y: box.y },
    { x: box.x + box.w, y: box.y },
    { x: box.x + box.w, y: box.y + box.h },
    { x: box.x, y: box.y + box.h },
  ];
  for (let i = 0; i < corners.length; i += 1) {
    const c = corners[i];
    if (!c) continue;
    if (Math.abs(p.x - c.x) <= tolerance && Math.abs(p.y - c.y) <= tolerance) {
      return i as 0 | 1 | 2 | 3;
    }
  }
  return null;
}

/** Whether a drag produced any actual geometry change (avoid empty history entries). */
export function layerChanged(a: EditorLayer, b: EditorLayer): boolean {
  if (a.x !== b.x || a.y !== b.y || a.w !== b.w || a.h !== b.h) return true;
  if ((a.x2 ?? -1) !== (b.x2 ?? -1)) return true;
  if ((a.y2 ?? -1) !== (b.y2 ?? -1)) return true;
  const pa = a.points;
  const pb = b.points;
  if ((pa === undefined) !== (pb === undefined)) return true;
  if (pa && pb) {
    if (pa.length !== pb.length) return true;
    for (let i = 0; i < pa.length; i += 1) {
      const qa = pa[i];
      const qb = pb[i];
      if (!qa || !qb || qa.x !== qb.x || qa.y !== qb.y) return true;
    }
  }
  return false;
}

/** Shift all coordinates of a layer by (dx, dy). Used when cropping the base image. */
export function shiftLayer(layer: EditorLayer, dx: number, dy: number): EditorLayer {
  const out = cloneLayer(layer);
  out.x += dx;
  out.y += dy;
  if (out.x2 !== undefined) out.x2 += dx;
  if (out.y2 !== undefined) out.y2 += dy;
  return out;
}

/** True when the layer's bounding box does not intersect the (0,0,w,h) base area at all. */
export function isLayerOutside(layer: EditorLayer, width: number, height: number): boolean {
  const box = layerBBox(layer);
  return box.x >= width || box.y >= height || box.x + box.w <= 0 || box.y + box.h <= 0;
}

interface ClampedRegion {
  x: number;
  y: number;
  w: number;
  h: number;
}

function clampRegion(
  x: number,
  y: number,
  w: number,
  h: number,
  baseW: number,
  baseH: number,
): ClampedRegion | null {
  const rx = Math.max(0, Math.floor(x));
  const ry = Math.max(0, Math.floor(y));
  const rw = Math.min(baseW, Math.ceil(x + w)) - rx;
  const rh = Math.min(baseH, Math.ceil(y + h)) - ry;
  if (rw <= 0 || rh <= 0) return null;
  return { x: rx, y: ry, w: rw, h: rh };
}

function makeCanvas(width: number, height: number): HTMLCanvasElement | null {
  if (width < 1 || height < 1) return null;
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  return canvas;
}

function getContext2d(canvas: HTMLCanvasElement): CanvasRenderingContext2D | null {
  return canvas.getContext('2d');
}

/**
 * Soft blur of a region of the BASE image, implemented with drawImage only
 * (ctx.filter is unsupported in macOS WebKit): downscale to ~1/10, upscale,
 * downscale again, upscale back while drawing onto the target.
 */
function blurRegion(
  ctx: CanvasRenderingContext2D,
  base: HTMLImageElement | HTMLCanvasElement,
  x: number,
  y: number,
  w: number,
  h: number,
  baseW: number,
  baseH: number,
): void {
  const region = clampRegion(x, y, w, h, baseW, baseH);
  if (!region) return;
  const dw = Math.max(1, Math.round(region.w * BLUR_DOWNSCALE));
  const dh = Math.max(1, Math.round(region.h * BLUR_DOWNSCALE));

  const tiny = makeCanvas(dw, dh);
  if (!tiny) return;
  const tctx = getContext2d(tiny);
  if (!tctx) return;
  tctx.imageSmoothingEnabled = true;
  tctx.imageSmoothingQuality = 'high';
  tctx.drawImage(base, region.x, region.y, region.w, region.h, 0, 0, dw, dh);

  const full = makeCanvas(region.w, region.h);
  if (!full) return;
  const fctx = getContext2d(full);
  if (!fctx) return;
  fctx.imageSmoothingEnabled = true;
  fctx.imageSmoothingQuality = 'high';
  fctx.drawImage(tiny, 0, 0, region.w, region.h);

  // Second down/up pass for extra softness.
  const tiny2 = makeCanvas(dw, dh);
  if (!tiny2) return;
  const t2ctx = getContext2d(tiny2);
  if (!t2ctx) return;
  t2ctx.imageSmoothingEnabled = true;
  t2ctx.imageSmoothingQuality = 'high';
  t2ctx.drawImage(full, 0, 0, dw, dh);

  ctx.save();
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = 'high';
  ctx.drawImage(tiny2, 0, 0, dw, dh, region.x, region.y, region.w, region.h);
  ctx.restore();
}

/** Pixelate a region of the BASE image with a fixed block factor, smoothing disabled. */
function pixelateRegion(
  ctx: CanvasRenderingContext2D,
  base: HTMLImageElement | HTMLCanvasElement,
  x: number,
  y: number,
  w: number,
  h: number,
  baseW: number,
  baseH: number,
): void {
  const region = clampRegion(x, y, w, h, baseW, baseH);
  if (!region) return;
  const dw = Math.max(1, Math.ceil(region.w / PIXELATE_BLOCK));
  const dh = Math.max(1, Math.ceil(region.h / PIXELATE_BLOCK));
  const tiny = makeCanvas(dw, dh);
  if (!tiny) return;
  const tctx = getContext2d(tiny);
  if (!tctx) return;
  tctx.imageSmoothingEnabled = false;
  tctx.drawImage(base, region.x, region.y, region.w, region.h, 0, 0, dw, dh);

  ctx.save();
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(tiny, 0, 0, dw, dh, region.x, region.y, region.w, region.h);
  ctx.restore();
}

function drawArrow(ctx: CanvasRenderingContext2D, layer: EditorLayer): void {
  const x2 = layer.x2 ?? layer.x;
  const y2 = layer.y2 ?? layer.y;
  const angle = Math.atan2(y2 - layer.y, x2 - layer.x);
  const headLength = Math.max(layer.strokeWidth * 3, 12);
  const spread = (Math.PI * 25) / 180;
  ctx.save();
  ctx.strokeStyle = layer.color;
  ctx.lineWidth = layer.strokeWidth;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  ctx.beginPath();
  ctx.moveTo(layer.x, layer.y);
  ctx.lineTo(x2, y2);
  ctx.moveTo(x2, y2);
  ctx.lineTo(
    x2 + headLength * Math.cos(angle + Math.PI - spread),
    y2 + headLength * Math.sin(angle + Math.PI - spread),
  );
  ctx.moveTo(x2, y2);
  ctx.lineTo(
    x2 + headLength * Math.cos(angle + Math.PI + spread),
    y2 + headLength * Math.sin(angle + Math.PI + spread),
  );
  ctx.stroke();
  ctx.restore();
}

function drawPen(ctx: CanvasRenderingContext2D, layer: EditorLayer): void {
  const pts = layer.points ?? [];
  const first = pts[0];
  if (!first) return;
  ctx.save();
  ctx.strokeStyle = layer.color;
  ctx.lineWidth = layer.strokeWidth;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  ctx.beginPath();
  ctx.moveTo(first.x, first.y);
  for (let i = 1; i < pts.length; i += 1) {
    const p = pts[i];
    if (p) ctx.lineTo(p.x, p.y);
  }
  ctx.stroke();
  ctx.restore();
}

function drawLayer(
  ctx: CanvasRenderingContext2D,
  layer: EditorLayer,
  base: HTMLImageElement | HTMLCanvasElement,
  baseW: number,
  baseH: number,
): void {
  switch (layer.kind) {
    case 'rect': {
      const box = normalizeBox(layer.x, layer.y, layer.w, layer.h);
      ctx.save();
      ctx.strokeStyle = layer.color;
      ctx.lineWidth = layer.strokeWidth;
      ctx.strokeRect(box.x, box.y, box.w, box.h);
      ctx.restore();
      break;
    }
    case 'ellipse': {
      const box = normalizeBox(layer.x, layer.y, layer.w, layer.h);
      ctx.save();
      ctx.strokeStyle = layer.color;
      ctx.lineWidth = layer.strokeWidth;
      ctx.beginPath();
      ctx.ellipse(box.x + box.w / 2, box.y + box.h / 2, box.w / 2, box.h / 2, 0, 0, Math.PI * 2);
      ctx.stroke();
      ctx.restore();
      break;
    }
    case 'arrow':
      drawArrow(ctx, layer);
      break;
    case 'pen':
      drawPen(ctx, layer);
      break;
    case 'text': {
      const size = textFontSize(layer);
      ctx.save();
      ctx.font = `${size}px ${FONT_STACK}`;
      ctx.fillStyle = layer.color;
      ctx.textBaseline = 'top';
      ctx.fillText(layer.text ?? '', layer.x, layer.y);
      ctx.restore();
      break;
    }
    case 'highlight': {
      const box = normalizeBox(layer.x, layer.y, layer.w, layer.h);
      ctx.save();
      ctx.globalCompositeOperation = 'multiply';
      ctx.globalAlpha = 0.4;
      ctx.fillStyle = layer.color;
      ctx.fillRect(box.x, box.y, box.w, box.h);
      ctx.restore();
      break;
    }
    case 'blur':
      blurRegion(ctx, base, layer.x, layer.y, layer.w, layer.h, baseW, baseH);
      break;
    case 'pixelate':
      pixelateRegion(ctx, base, layer.x, layer.y, layer.w, layer.h, baseW, baseH);
      break;
    case 'number': {
      ctx.save();
      ctx.beginPath();
      ctx.arc(layer.x, layer.y, NUMBER_RADIUS, 0, Math.PI * 2);
      ctx.fillStyle = layer.color;
      ctx.fill();
      ctx.fillStyle = '#ffffff';
      ctx.font = `bold 16px ${FONT_STACK}`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(String(layer.badge ?? 1), layer.x, layer.y);
      ctx.restore();
      break;
    }
  }
}

function drawSelection(ctx: CanvasRenderingContext2D, box: EditorBox, scale: number): void {
  const lineWidth = Math.max(1, 1.5 / scale);
  const handleSize = Math.max(4, 8 / scale);
  const corners: EditorPoint[] = [
    { x: box.x, y: box.y },
    { x: box.x + box.w, y: box.y },
    { x: box.x + box.w, y: box.y + box.h },
    { x: box.x, y: box.y + box.h },
  ];
  ctx.save();
  ctx.strokeStyle = SELECTION_COLOR;
  ctx.lineWidth = lineWidth;
  ctx.setLineDash([5 / scale, 4 / scale]);
  ctx.strokeRect(box.x, box.y, box.w, box.h);
  ctx.setLineDash([]);
  ctx.lineWidth = Math.max(1, 1 / scale);
  for (const c of corners) {
    ctx.fillStyle = '#fafafa';
    ctx.fillRect(c.x - handleSize / 2, c.y - handleSize / 2, handleSize, handleSize);
    ctx.strokeRect(c.x - handleSize / 2, c.y - handleSize / 2, handleSize, handleSize);
  }
  ctx.restore();
}

function drawCropOverlay(
  ctx: CanvasRenderingContext2D,
  box: EditorBox,
  baseW: number,
  baseH: number,
  scale: number,
): void {
  const right = box.x + box.w;
  const bottom = box.y + box.h;
  ctx.save();
  ctx.fillStyle = 'rgba(0, 0, 0, 0.45)';
  ctx.fillRect(0, 0, baseW, Math.max(0, box.y));
  ctx.fillRect(0, bottom, baseW, Math.max(0, baseH - bottom));
  ctx.fillRect(0, box.y, Math.max(0, box.x), box.h);
  ctx.fillRect(right, box.y, Math.max(0, baseW - right), box.h);
  ctx.strokeStyle = SELECTION_COLOR;
  ctx.lineWidth = Math.max(1, 1.5 / scale);
  ctx.setLineDash([]);
  ctx.strokeRect(box.x, box.y, box.w, box.h);
  ctx.restore();
}

/**
 * Draw a full editor scene: base image, then layers in order (skipping the
 * draft twin of a layer being edited, drawing the draft on top), then the
 * crop overlay, then the selection UI.
 */
export function renderScene(ctx: CanvasRenderingContext2D, scene: SceneState): void {
  ctx.save();
  ctx.clearRect(0, 0, scene.baseW, scene.baseH);
  ctx.drawImage(scene.baseImage, 0, 0);
  for (const layer of scene.layers) {
    if (scene.draft && layer.id === scene.draft.id) continue;
    drawLayer(ctx, layer, scene.baseImage, scene.baseW, scene.baseH);
  }
  if (scene.draft) {
    drawLayer(ctx, scene.draft, scene.baseImage, scene.baseW, scene.baseH);
  }
  if (scene.cropRect && scene.cropRect.w > 0 && scene.cropRect.h > 0) {
    drawCropOverlay(ctx, scene.cropRect, scene.baseW, scene.baseH, scene.scale);
  }
  const selected =
    scene.draft && scene.draft.id === scene.selectedId
      ? scene.draft
      : scene.selectedId
        ? scene.layers.find((l) => l.id === scene.selectedId)
        : undefined;
  if (selected) drawSelection(ctx, layerBBox(selected), scene.scale);
  ctx.restore();
}

/** Render base + layers at natural size onto an offscreen canvas (for export). */
export function buildExportCanvas(doc: EditorDoc): HTMLCanvasElement {
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, doc.baseW);
  canvas.height = Math.max(1, doc.baseH);
  const ctx = getContext2d(canvas);
  if (ctx) {
    renderScene(ctx, {
      baseImage: doc.baseImage,
      baseW: doc.baseW,
      baseH: doc.baseH,
      layers: doc.layers,
      draft: null,
      selectedId: null,
      cropRect: null,
      scale: 1,
    });
  }
  return canvas;
}
