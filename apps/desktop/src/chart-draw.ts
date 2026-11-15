import type { ChartInfo } from "./engine";

/** Cores das séries, as mesmas do arquivo gravado. */
export const SERIES_COLORS = ["#4F46E5", "#F59E0B", "#10B981", "#EF4444", "#3B82F6", "#8B5CF6", "#EC4899", "#14B8A6"];

const INK = "#1b1b29";
const MUTED = "#5b5b6b";
const LINE = "#e4e4ec";
const ACCENT = "#4f46e5";
const FONT = "Inter, system-ui, sans-serif";
/** Lado, em pixels, da alça de redimensionar. */
export const CHART_HANDLE = 9;

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

const numbers = new Intl.NumberFormat("pt-BR", { maximumFractionDigits: 2 });
const compact = new Intl.NumberFormat("pt-BR", { notation: "compact", maximumFractionDigits: 1 });

/** Rótulo de um valor do eixo: números grandes viram `1,5 mi`, `20 mil`. */
function axisLabel(value: number): string {
  return Math.abs(value) >= 10_000 ? compact.format(value) : numbers.format(value);
}

/** A cor da série `index`: a escolhida ou a da paleta. */
export function seriesColor(chart: Pick<ChartInfo, "styles">, index: number): string {
  return chart.styles[index]?.color ?? SERIES_COLORS[index % SERIES_COLORS.length];
}

/** Um passo "redondo" (1, 2, 5 vezes uma potência de dez) para dividir `range` em uns `ticks` trechos. */
function niceStep(range: number, ticks: number): number {
  const raw = range / ticks;
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const fraction = raw / magnitude;
  return (fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 5 ? 5 : 10) * magnitude;
}

export interface Scale {
  min: number;
  max: number;
  step: number;
}

/**
 * Escala de um eixo de valores. Os extremos caem em múltiplos do passo, a menos
 * que `lo`/`hi` os fixem; `zero` obriga a incluir o zero.
 */
export function axisScale(values: number[], lo: number | null, hi: number | null, zero = true): Scale {
  let min = values.length ? Math.min(...values) : 0;
  let max = values.length ? Math.max(...values) : 1;
  if (zero) {
    min = Math.min(0, min);
    max = Math.max(0, max);
  }
  if (lo !== null) min = lo;
  if (hi !== null) max = hi;
  if (!Number.isFinite(min) || !Number.isFinite(max) || min >= max) {
    min = lo ?? 0;
    max = hi !== null && hi > min ? hi : min + 1;
  }
  const step = niceStep(max - min, 5);
  return {
    min: lo ?? Math.floor(min / step + 1e-9) * step,
    max: hi ?? Math.ceil(max / step - 1e-9) * step,
    step,
  };
}

/** Compatível com o nome antigo: escala que sempre inclui o zero. */
export function niceScale(values: number[], ticks = 5): Scale {
  void ticks;
  return axisScale(values, null, null);
}

function ticksOf(scale: Scale): number[] {
  const out: number[] = [];
  for (let k = 0; scale.min + k * scale.step <= scale.max + scale.step / 2 && k < 200; k++) {
    out.push(Math.round((scale.min + k * scale.step) * 1e9) / 1e9);
  }
  return out;
}

/** O texto cortado com reticências até caber em `width`. */
function fit(ctx: CanvasRenderingContext2D, text: string, width: number): string {
  if (ctx.measureText(text).width <= width) return text;
  let end = text.length;
  while (end > 1 && ctx.measureText(`${text.slice(0, end)}…`).width > width) end--;
  return `${text.slice(0, end)}…`;
}

type LegendSide = "none" | "bottom" | "top" | "left" | "right";

function legendSide(chart: ChartInfo): LegendSide {
  const round = chart.kind === "pie" || chart.kind === "donut";
  if (chart.legend === "auto") return round || chart.data.series.length > 1 ? "bottom" : "none";
  return chart.legend;
}

/** Os nomes da legenda: as categorias na pizza e na rosca, as séries nos demais. */
function legendNames(chart: ChartInfo): string[] {
  return chart.kind === "pie" || chart.kind === "donut" ? chart.data.categories : chart.data.series.map((one) => one.name);
}

function legendColors(chart: ChartInfo): string[] {
  if (chart.kind === "pie" || chart.kind === "donut") return SERIES_COLORS;
  return chart.data.series.map((_, index) => seriesColor(chart, index));
}

function legendWidth(ctx: CanvasRenderingContext2D, names: string[], limit: number): number {
  ctx.font = `11px ${FONT}`;
  return Math.min(limit, 36 + Math.max(0, ...names.map((name) => ctx.measureText(name).width)));
}

function drawLegend(ctx: CanvasRenderingContext2D, chart: ChartInfo, side: LegendSide, box: Box) {
  const names = legendNames(chart);
  const colors = legendColors(chart);
  ctx.font = `11px ${FONT}`;
  ctx.textBaseline = "middle";
  ctx.textAlign = "left";
  if (side === "left" || side === "right") {
    const rows = Math.min(names.length, Math.max(1, Math.floor((box.height - 8) / 18)));
    let y = box.top + (box.height - rows * 18) / 2 + 9;
    for (let index = 0; index < rows; index++) {
      ctx.fillStyle = colors[index % colors.length];
      ctx.fillRect(box.left + 8, y - 4, 8, 8);
      ctx.fillStyle = MUTED;
      ctx.fillText(fit(ctx, names[index], box.width - 26), box.left + 21, y);
      y += 18;
    }
    return;
  }
  const items = names.map((name) => ({ name: fit(ctx, name, 120), width: 0 }));
  for (const item of items) item.width = 14 + ctx.measureText(item.name).width + 14;
  const total = items.reduce((sum, item) => sum + item.width, 0);
  let x = box.left + Math.max(8, (box.width - total) / 2);
  const y = box.top + box.height / 2;
  items.forEach((item, index) => {
    if (x + item.width > box.left + box.width) return;
    ctx.fillStyle = colors[index % colors.length];
    ctx.fillRect(x, y - 4, 8, 8);
    ctx.fillStyle = MUTED;
    ctx.fillText(item.name, x + 13, y);
    x += item.width;
  });
}

// ---------------------------------------------------------------------------
// Eixos cartesianos
// ---------------------------------------------------------------------------

type Role = "bar" | "line" | "area" | "scatter";

function roleOf(chart: ChartInfo, series: number): Role {
  switch (chart.kind) {
    case "column":
      return chart.styles[series]?.line ? "line" : "bar";
    case "bar":
      return "bar";
    case "line":
      return "line";
    case "area":
      return "area";
    default:
      return "scatter";
  }
}

const isSecondary = (chart: ChartInfo, series: number) => Boolean(chart.styles[series]?.secondary);

interface Span {
  lo: number;
  hi: number;
}

/** Para cada série e categoria, de onde a barra ou a área sai e até onde vai (empilhamento incluído). */
function spansOf(chart: ChartInfo): (Span | null)[][] {
  const { series, categories } = chart.data;
  const result: (Span | null)[][] = series.map(() => categories.map(() => null));
  const stacked = chart.stacking !== "none";
  for (let i = 0; i < categories.length; i++) {
    const groups = new Map<string, { positive: number; negative: number; total: number }>();
    series.forEach((one, s) => {
      const role = roleOf(chart, s);
      const stackable = stacked && role !== "scatter" && !(chart.kind === "column" && role === "line");
      const value = one.values[i];
      if (value === null || value === undefined) return;
      if (!stackable) {
        result[s][i] = { lo: role === "line" ? value : 0, hi: value };
        return;
      }
      const key = `${isSecondary(chart, s)}|${role}`;
      if (!groups.has(key)) {
        let total = 0;
        series.forEach((other, o) => {
          if (`${isSecondary(chart, o)}|${roleOf(chart, o)}` === key) total += Math.abs(other.values[i] ?? 0);
        });
        groups.set(key, { positive: 0, negative: 0, total });
      }
      const group = groups.get(key)!;
      const scaled = chart.stacking === "percent" ? (group.total > 0 ? value / group.total : 0) : value;
      if (scaled >= 0) {
        result[s][i] = { lo: group.positive, hi: group.positive + scaled };
        group.positive += scaled;
      } else {
        result[s][i] = { lo: group.negative + scaled, hi: group.negative };
        group.negative += scaled;
      }
    });
  }
  return result;
}

function drawCartesian(ctx: CanvasRenderingContext2D, chart: ChartInfo, box: Box) {
  const { categories, series } = chart.data;
  const scatter = chart.kind === "scatter";
  const horizontal = chart.kind === "bar";
  const percent = chart.stacking === "percent";
  const spans = spansOf(chart);
  const count = categories.length;
  if (count === 0 || series.length === 0) return;

  // Escalas: a principal (com os limites escolhidos) e, se houver, a secundária.
  const valuesOf = (secondary: boolean) => {
    const out: number[] = [];
    series.forEach((_, s) => {
      if (isSecondary(chart, s) !== secondary) return;
      for (const span of spans[s]) {
        if (span) out.push(span.lo, span.hi);
      }
    });
    return out;
  };
  const hasSecondary = series.some((_, s) => isSecondary(chart, s));
  const primary: Scale = percent ? { min: 0, max: 1, step: 0.2 } : axisScale(valuesOf(false), chart.yMin, chart.yMax);
  const secondary: Scale | null = hasSecondary ? axisScale(valuesOf(true), null, null) : null;
  const xs = chart.data.x.filter((x): x is number => x !== null);
  const xScale: Scale | null = scatter ? axisScale(xs, null, null, false) : null;

  const format = (value: number) => (percent ? `${Math.round(value * 100)}%` : axisLabel(value));
  ctx.font = `10px ${FONT}`;
  const primaryTicks = ticksOf(primary);
  const widest = (labels: string[]) => Math.max(0, ...labels.map((label) => ctx.measureText(label).width));
  const valueLabelWidth = widest(primaryTicks.map(format));
  const secondaryTicks = secondary ? ticksOf(secondary) : [];
  const secondaryWidth = secondary ? widest(secondaryTicks.map(axisLabel)) : 0;
  const categoryWidth = horizontal ? Math.min(110, widest(categories)) : 0;

  const leftTitle = horizontal ? chart.xTitle : chart.yTitle;
  const bottomTitle = horizontal ? chart.yTitle : chart.xTitle;
  const left = box.left + (horizontal ? categoryWidth + 10 : valueLabelWidth + 10) + (leftTitle ? 16 : 0);
  const right = box.left + box.width - (secondary ? secondaryWidth + 12 : horizontal ? 18 : 12);
  const top = box.top + 14;
  const bottom = box.top + box.height - (horizontal ? 20 : 22) - (bottomTitle ? 16 : 0);
  const plotW = right - left;
  const plotH = bottom - top;
  if (plotW < 20 || plotH < 20) return;

  const scaleOf = (s: number) => (isSecondary(chart, s) && secondary ? secondary : primary);
  const valuePx = (value: number, scale: Scale) =>
    horizontal ? left + ((value - scale.min) / (scale.max - scale.min)) * plotW : bottom - ((value - scale.min) / (scale.max - scale.min)) * plotH;
  const edges = chart.kind === "area" && count > 1;
  const slot = (horizontal ? plotH : plotW) / count;
  /** Posição, no eixo das categorias, da categoria `position` (a partir de zero; pode ser fracionária). */
  const categoryPx = (position: number) => {
    const start = horizontal ? top : left;
    const length = horizontal ? plotH : plotW;
    return edges ? start + (length * position) / (count - 1) : start + slot * (position + 0.5);
  };
  const xPx = (x: number) => left + ((x - xScale!.min) / (xScale!.max - xScale!.min)) * plotW;

  // Linhas de grade e rótulos do eixo de valores.
  ctx.lineWidth = 1;
  primaryTicks.forEach((value) => {
    const at = Math.round(valuePx(value, primary)) + 0.5;
    ctx.strokeStyle = LINE;
    ctx.fillStyle = MUTED;
    ctx.beginPath();
    if (horizontal) {
      if (chart.grid) {
        ctx.moveTo(at, top);
        ctx.lineTo(at, bottom);
      }
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      ctx.fillText(format(value), at, bottom + 5);
    } else {
      if (chart.grid) {
        ctx.moveTo(left, at);
        ctx.lineTo(right, at);
      }
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      ctx.fillText(format(value), left - 6, at);
    }
    ctx.stroke();
  });
  if (secondary) {
    ctx.fillStyle = MUTED;
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";
    for (const value of secondaryTicks) ctx.fillText(axisLabel(value), right + 6, Math.round(valuePx(value, secondary)) + 0.5);
  }

  // Eixo das categorias (ou dos valores de X, na dispersão).
  ctx.fillStyle = MUTED;
  if (scatter && xScale) {
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    for (const value of ticksOf(xScale)) {
      const at = Math.round(xPx(value)) + 0.5;
      ctx.strokeStyle = LINE;
      if (chart.grid) {
        ctx.beginPath();
        ctx.moveTo(at, top);
        ctx.lineTo(at, bottom);
        ctx.stroke();
      }
      ctx.fillStyle = MUTED;
      ctx.fillText(axisLabel(value), at, bottom + 6);
    }
  } else {
    const every = Math.max(1, Math.ceil(24 / Math.max(1, slot)));
    categories.forEach((category, index) => {
      if (index % every !== 0) return;
      const center = categoryPx(index);
      if (horizontal) {
        ctx.textAlign = "right";
        ctx.textBaseline = "middle";
        ctx.fillText(fit(ctx, category, categoryWidth), left - 6, center);
      } else {
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        ctx.fillText(fit(ctx, category, slot * every - 4), center, bottom + 6);
      }
    });
  }
  // Linha do zero.
  ctx.strokeStyle = "#9d9db5";
  ctx.beginPath();
  if (horizontal) {
    const zero = Math.round(valuePx(0, primary)) + 0.5;
    ctx.moveTo(zero, top);
    ctx.lineTo(zero, bottom);
  } else {
    const zero = Math.round(valuePx(Math.max(primary.min, Math.min(0, primary.max)), primary)) + 0.5;
    ctx.moveTo(left, zero);
    ctx.lineTo(right, zero);
  }
  ctx.stroke();

  // Áreas, depois barras, linhas e pontos.
  const point = (s: number, i: number, value: number): [number, number] => {
    const along = scatter ? xPx(chart.data.x[i] ?? i + 1) : categoryPx(i);
    return horizontal ? [valuePx(value, scaleOf(s)), along] : [along, valuePx(value, scaleOf(s))];
  };
  series.forEach((_, s) => {
    if (roleOf(chart, s) !== "area") return;
    ctx.fillStyle = seriesColor(chart, s);
    ctx.globalAlpha = chart.stacking === "none" ? 0.55 : 0.9;
    ctx.beginPath();
    let run: number[] = [];
    const flush = () => {
      if (run.length === 0) return;
      ctx.beginPath();
      run.forEach((i, k) => {
        const [x, y] = point(s, i, spans[s][i]!.hi);
        if (k === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      });
      [...run].reverse().forEach((i) => {
        const [x, y] = point(s, i, spans[s][i]!.lo);
        ctx.lineTo(x, y);
      });
      ctx.closePath();
      ctx.fill();
      run = [];
    };
    for (let i = 0; i < count; i++) {
      if (spans[s][i]) run.push(i);
      else flush();
    }
    flush();
    ctx.globalAlpha = 1;
  });

  const barSeries = series.map((_, s) => s).filter((s) => roleOf(chart, s) === "bar");
  const stackedBars = chart.stacking !== "none";
  const groupSize = slot * 0.72;
  const each = groupSize / Math.max(1, stackedBars ? 1 : barSeries.length);
  for (const s of barSeries) {
    ctx.fillStyle = seriesColor(chart, s);
    const order = stackedBars ? 0 : barSeries.indexOf(s);
    for (let i = 0; i < count; i++) {
      const span = spans[s][i];
      if (!span) continue;
      const offset = (horizontal ? top : left) + slot * i + (slot - groupSize) / 2 + each * order;
      const from = valuePx(span.lo, scaleOf(s));
      const to = valuePx(span.hi, scaleOf(s));
      if (horizontal) ctx.fillRect(Math.min(from, to), offset, Math.max(1, Math.abs(to - from)), Math.max(1, each - 1));
      else ctx.fillRect(offset, Math.min(from, to), Math.max(1, each - 1), Math.max(1, Math.abs(to - from)));
    }
  }

  series.forEach((one, s) => {
    const role = roleOf(chart, s);
    if (role !== "line" && role !== "scatter") return;
    const color = seriesColor(chart, s);
    const points: [number, number][] = [];
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.lineWidth = 2;
    ctx.lineJoin = "round";
    if (role === "line") {
      ctx.beginPath();
      let pen = false;
      let previous: [number, number] | null = null;
      one.values.forEach((_, i) => {
        const span = spans[s][i];
        if (!span) {
          pen = false;
          previous = null;
          return;
        }
        const [x, y] = point(s, i, span.hi);
        points.push([x, y]);
        if (pen && previous) {
          if (chart.smooth) {
            const midX = (previous[0] + x) / 2;
            const midY = (previous[1] + y) / 2;
            ctx.quadraticCurveTo(previous[0], previous[1], midX, midY);
            ctx.lineTo(x, y);
          } else ctx.lineTo(x, y);
        } else ctx.moveTo(x, y);
        pen = true;
        previous = [x, y];
      });
      ctx.stroke();
    } else {
      one.values.forEach((value, i) => {
        if (value === null || chart.data.x[i] === null) return;
        points.push(point(s, i, value));
      });
    }
    for (const [x, y] of points) {
      ctx.beginPath();
      ctx.arc(x, y, role === "scatter" ? 4 : 3, 0, Math.PI * 2);
      ctx.fill();
    }
  });

  // Linhas de tendência, tracejadas na cor da série.
  const notes: { text: string; color: string }[] = [];
  series.forEach((one, s) => {
    if (!one.trend || one.trend.points.length === 0) return;
    const color = seriesColor(chart, s);
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.setLineDash([5, 4]);
    ctx.beginPath();
    one.trend.points.forEach(([tx, ty], k) => {
      const along = scatter ? xPx(tx) : categoryPx(tx - 1);
      const x = horizontal ? valuePx(ty, scaleOf(s)) : along;
      const y = horizontal ? along : valuePx(ty, scaleOf(s));
      if (k === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
    ctx.setLineDash([]);
    if (one.trend.label) notes.push({ text: one.trend.label, color });
  });
  if (notes.length) {
    ctx.font = `10px ${FONT}`;
    ctx.textAlign = "right";
    ctx.textBaseline = "top";
    let y = top + 4;
    for (const note of notes) {
      ctx.fillStyle = note.color;
      for (const line of note.text.split("\n")) {
        ctx.fillText(line, right - 4, y);
        y += 12;
      }
    }
  }

  // Rótulos de dados.
  if (chart.labels) {
    ctx.font = `10px ${FONT}`;
    ctx.fillStyle = INK;
    series.forEach((one, s) => {
      const role = roleOf(chart, s);
      one.values.forEach((value, i) => {
        const span = spans[s][i];
        if (value === null || !span) return;
        const text = numbers.format(value);
        const shown = role === "bar" && chart.stacking !== "none" ? (span.lo + span.hi) / 2 : span.hi;
        const [x, y] = point(s, i, shown);
        if (horizontal) {
          ctx.textAlign = chart.stacking !== "none" ? "center" : "left";
          ctx.textBaseline = "middle";
          ctx.fillText(text, chart.stacking !== "none" ? x : x + 4, y);
        } else {
          ctx.textAlign = "center";
          ctx.textBaseline = chart.stacking !== "none" && role === "bar" ? "middle" : value < 0 ? "top" : "bottom";
          ctx.fillText(text, x, chart.stacking !== "none" && role === "bar" ? y : y + (value < 0 ? 4 : -4));
        }
      });
    });
  }

  // Títulos dos eixos.
  ctx.font = `10px ${FONT}`;
  ctx.fillStyle = MUTED;
  if (bottomTitle) {
    ctx.textAlign = "center";
    ctx.textBaseline = "bottom";
    ctx.fillText(fit(ctx, bottomTitle, plotW), left + plotW / 2, box.top + box.height - 2);
  }
  if (leftTitle) {
    ctx.save();
    ctx.translate(box.left + 11, top + plotH / 2);
    ctx.rotate(-Math.PI / 2);
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(fit(ctx, leftTitle, plotH), 0, 0);
    ctx.restore();
  }
}

// ---------------------------------------------------------------------------
// Pizza e rosca
// ---------------------------------------------------------------------------

/** Pizza: a primeira série; rosca: um anel por série, o primeiro no centro. */
function drawRound(ctx: CanvasRenderingContext2D, chart: ChartInfo, box: Box) {
  const donut = chart.kind === "donut";
  const rings = donut ? chart.data.series : chart.data.series.slice(0, 1);
  const radius = Math.min(box.width, box.height) / 2 - 10;
  if (rings.length === 0 || radius < 10) return;
  const cx = box.left + box.width / 2;
  const cy = box.top + box.height / 2;
  const hole = donut ? radius * 0.45 : 0;
  const thickness = (radius - hole) / rings.length;
  ctx.font = `bold 10px ${FONT}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  rings.forEach((ring, r) => {
    const values = ring.values.map((value) => (value !== null && value > 0 ? value : 0));
    const total = values.reduce((sum, value) => sum + value, 0);
    if (total <= 0) return;
    const inner = hole + thickness * r;
    const outer = inner + thickness;
    let angle = -Math.PI / 2;
    values.forEach((value, index) => {
      if (value === 0) return;
      const sweep = (value / total) * Math.PI * 2;
      ctx.beginPath();
      if (inner > 0) {
        ctx.arc(cx, cy, outer, angle, angle + sweep);
        ctx.arc(cx, cy, inner, angle + sweep, angle, true);
      } else {
        ctx.moveTo(cx, cy);
        ctx.arc(cx, cy, outer, angle, angle + sweep);
      }
      ctx.closePath();
      ctx.fillStyle = SERIES_COLORS[index % SERIES_COLORS.length];
      ctx.fill();
      ctx.strokeStyle = "#fff";
      ctx.lineWidth = 1.5;
      ctx.stroke();
      if (chart.labels && sweep > 0.2) {
        const middle = angle + sweep / 2;
        const at = inner > 0 ? (inner + outer) / 2 : outer * 0.65;
        ctx.fillStyle = "#fff";
        ctx.fillText(`${Math.round((value / total) * 100)}%`, cx + Math.cos(middle) * at, cy + Math.sin(middle) * at);
      }
      angle += sweep;
    });
  });
}

/** Desenha o gráfico dentro de `box`; selecionado, com contorno e alça de redimensionar. */
export function drawChart(ctx: CanvasRenderingContext2D, chart: ChartInfo, box: Box, selected: boolean) {
  ctx.save();
  ctx.beginPath();
  ctx.rect(box.left, box.top, box.width, box.height);
  ctx.clip();
  ctx.fillStyle = "#fff";
  ctx.fillRect(box.left, box.top, box.width, box.height);

  let top = box.top;
  if (chart.title) {
    ctx.font = `600 13px ${FONT}`;
    ctx.fillStyle = INK;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(fit(ctx, chart.title, box.width - 24), box.left + box.width / 2, box.top + 20);
    top += 36;
  }
  const side = legendSide(chart);
  const stripe = 24;
  const sideWidth = side === "left" || side === "right" ? legendWidth(ctx, legendNames(chart), box.width * 0.35) : 0;
  const plot: Box = {
    left: box.left + (side === "left" ? sideWidth : 0),
    top: top + (side === "top" ? stripe : 0),
    width: box.width - sideWidth,
    height: box.top + box.height - top - (side === "top" || side === "bottom" ? stripe : 0),
  };
  if (plot.width >= 80 && plot.height >= 60) {
    if (chart.kind === "pie" || chart.kind === "donut") drawRound(ctx, chart, plot);
    else drawCartesian(ctx, chart, plot);
    if (side !== "none") {
      const legendBox: Box =
        side === "bottom"
          ? { left: box.left, top: box.top + box.height - stripe, width: box.width, height: stripe }
          : side === "top"
            ? { left: box.left, top, width: box.width, height: stripe }
            : side === "left"
              ? { left: box.left, top, width: sideWidth, height: box.top + box.height - top }
              : { left: box.left + box.width - sideWidth, top, width: sideWidth, height: box.top + box.height - top };
      drawLegend(ctx, chart, side, legendBox);
    }
  }
  ctx.restore();

  ctx.strokeStyle = selected ? ACCENT : "#c9c9d6";
  ctx.lineWidth = selected ? 2 : 1;
  ctx.strokeRect(box.left + 0.5, box.top + 0.5, box.width - 1, box.height - 1);
  if (selected) {
    ctx.fillStyle = "#fff";
    ctx.fillRect(box.left + box.width - CHART_HANDLE, box.top + box.height - CHART_HANDLE, CHART_HANDLE, CHART_HANDLE);
    ctx.strokeRect(box.left + box.width - CHART_HANDLE + 0.5, box.top + box.height - CHART_HANDLE + 0.5, CHART_HANDLE - 1, CHART_HANDLE - 1);
  }
}
