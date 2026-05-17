export interface MiddleTruncateOptions {
  font?: string;
  ellipsis?: string;
  minHeadChars?: number;
  minTailChars?: number;
}

type Ctx2D = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

const DEFAULT_FONT = '0.75rem ui-monospace, SFMono-Regular, monospace';
const DEFAULT_ELLIPSIS = '…';
const DEFAULT_MIN_HEAD = 6;
const DEFAULT_MIN_TAIL = 8;
const FALLBACK_CHAR_WIDTH = 7;

const measureCache: WeakMap<object, Map<string, Map<string, number>>> = new WeakMap();

let resolvedCtx: Ctx2D | null | undefined;

function getContext(): Ctx2D | null {
  if (resolvedCtx !== undefined) return resolvedCtx;
  if (typeof document === 'undefined') {
    resolvedCtx = null;
    return null;
  }
  try {
    if (typeof OffscreenCanvas !== 'undefined') {
      const off = new OffscreenCanvas(1, 1);
      const ctx = off.getContext('2d');
      if (ctx) {
        resolvedCtx = ctx as OffscreenCanvasRenderingContext2D;
        return resolvedCtx;
      }
    }
    const canvas = document.createElement('canvas');
    const ctx = canvas.getContext('2d');
    resolvedCtx = ctx ?? null;
    return resolvedCtx;
  } catch {
    resolvedCtx = null;
    return resolvedCtx;
  }
}

export function measureText(text: string, font: string): number {
  if (text.length === 0) return 0;
  const ctx = getContext();
  if (!ctx) return text.length * FALLBACK_CHAR_WIDTH;
  let fontMap = measureCache.get(ctx as unknown as object);
  if (!fontMap) {
    fontMap = new Map<string, Map<string, number>>();
    measureCache.set(ctx as unknown as object, fontMap);
  }
  let textMap = fontMap.get(font);
  if (!textMap) {
    textMap = new Map<string, number>();
    fontMap.set(font, textMap);
  }
  const cached = textMap.get(text);
  if (cached !== undefined) return cached;
  try {
    if (ctx.font !== font) ctx.font = font;
    const width = ctx.measureText(text).width;
    textMap.set(text, width);
    return width;
  } catch {
    const fallback = text.length * FALLBACK_CHAR_WIDTH;
    textMap.set(text, fallback);
    return fallback;
  }
}

export function middleTruncateForWidth(
  text: string,
  availableWidthPx: number,
  options?: MiddleTruncateOptions,
): string {
  try {
    const font = options?.font ?? DEFAULT_FONT;
    const ellipsis = options?.ellipsis ?? DEFAULT_ELLIPSIS;
    const minHead = Math.max(0, options?.minHeadChars ?? DEFAULT_MIN_HEAD);
    const minTail = Math.max(0, options?.minTailChars ?? DEFAULT_MIN_TAIL);

    if (text.length === 0) return text;
    if (!Number.isFinite(availableWidthPx) || availableWidthPx <= 0) return ellipsis;

    const fullWidth = measureText(text, font);
    if (fullWidth <= availableWidthPx) return text;

    const len = text.length;
    let lo = 0;
    let hi = len;
    let best = -1;

    while (lo <= hi) {
      const mid = (lo + hi) >>> 1;
      const head = Math.ceil(mid / 2);
      const tail = Math.floor(mid / 2);
      const candidate = text.slice(0, head) + ellipsis + text.slice(len - tail);
      if (measureText(candidate, font) <= availableWidthPx) {
        best = mid;
        lo = mid + 1;
      } else {
        hi = mid - 1;
      }
    }

    if (best <= 0) return ellipsis;

    const head = Math.ceil(best / 2);
    const tail = Math.floor(best / 2);

    if (head < minHead || tail < minTail) {
      const safeHead = Math.min(minHead, len);
      const safeTail = Math.min(minTail, Math.max(0, len - safeHead));
      return text.slice(0, safeHead) + ellipsis + text.slice(len - safeTail);
    }

    return text.slice(0, head) + ellipsis + text.slice(len - tail);
  } catch {
    return text;
  }
}
