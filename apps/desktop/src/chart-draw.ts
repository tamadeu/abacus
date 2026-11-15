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

/** Um passo "redondo" (1, 2, 5 vezes uma potência de dez) para dividir `range` em uns `ticks` trechos. */
function niceStep(range: number, ticks: number): number {
  const raw = range / ticks;
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const fraction = raw / magnitude;
  return (fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 5 ? 5 : 10) * magnitude;
}

/** Escala do eixo de valores: sempre inclui o zero, e os extremos caem em múltiplos do passo. */
export function niceScale(values: number[], ticks = 5): { min: number; max: number; step: number } {
  let min = Math.min(0, ...values);
  let max = Math.max(0, ...values);
  if (!Number.isFinite(min) || !Number.isFinite(max) || min === max) {
    min = 0;
    max = 1;
  }
  const step = niceStep(max - min, ticks);
  return { min: Math.floor(min / step) * step, max: Math.ceil(max / step) * step, step };
}

/** O texto cortado com reticências até caber em `width`. */
function fit(ctx: CanvasRenderingContext2D, text: string, width: number): string {
  if (ctx.measureText(text).width <= width) return text;
  let end = text.length;
  while (end > 1 && ctx.measureText(`${text.slice(0, end)}…`).width > width) end--;
  return `${text.slice(0, end)}…`;
}

function legend(ctx: CanvasRenderingContext2D, names: string[], colors: string[], box: Box) {
  ctx.font = `11px ${FONT}`;
  ctx.textBaseline = "middle";
  ctx.textAlign = "left";
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

/** Colunas, barras ou linhas: a parte com eixos. */
function drawAxes(ctx: CanvasRenderingContext2D, chart: ChartInfo, box: Box) {
  const { categories, series } = chart.data;
  const horizontal = chart.kind === "bar";
  const all = series.flatMap((one) => one.values.filter((value): value is number => value !== null));
  const scale = niceScale(all);
  const span = scale.max - scale.min;

  ctx.font = `10px ${FONT}`;
  const ticks: number[] = [];
  for (let value = scale.min; value <= scale.max + scale.step / 2; value += scale.step) ticks.push(value);
  const labels = ticks.map(axisLabel);
  const labelWidth = Math.max(...labels.map((label) => ctx.measureText(label).width));

  // Espaço dos rótulos: os de valor num eixo, os de categoria no outro.
  const categoryWidth = horizontal
    ? Math.min(110, Math.max(0, ...categories.map((category) => ctx.measureText(category).width)))
    : 0;
  const left = box.left + (horizontal ? categoryWidth + 10 : labelWidth + 10);
  const right = box.left + box.width - (horizontal ? 18 : 12);
  const top = box.top + 14;
  const bottom = box.top + box.height - (horizontal ? 20 : 22);
  const plotW = right - left;
  const plotH = bottom - top;
  if (plotW < 20 || plotH < 20 || categories.length === 0) return;

  const toValue = (value: number) =>
    horizontal ? left + ((value - scale.min) / span) * plotW : bottom - ((value - scale.min) / span) * plotH;

  // Linhas de grade e rótulos do eixo de valores.
  ctx.strokeStyle = LINE;
  ctx.lineWidth = 1;
  ctx.fillStyle = MUTED;
  ctx.textBaseline = "middle";
  ticks.forEach((value, index) => {
    const at = Math.round(toValue(value)) + 0.5;
    ctx.beginPath();
    if (horizontal) {
      ctx.moveTo(at, top);
      ctx.lineTo(at, bottom);
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      ctx.fillText(labels[index], at, bottom + 5);
    } else {
      ctx.moveTo(left, at);
      ctx.lineTo(right, at);
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      ctx.fillText(labels[index], left - 6, at);
    }
    ctx.stroke();
  });

  // Eixo das categorias.
  const count = categories.length;
  const slot = (horizontal ? plotH : plotW) / count;
  ctx.fillStyle = MUTED;
  categories.forEach((category, index) => {
    const center = (horizontal ? top : left) + slot * (index + 0.5);
    if (horizontal) {
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      ctx.fillText(fit(ctx, category, categoryWidth), left - 6, center);
    } else {
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      ctx.fillText(fit(ctx, category, slot - 4), center, bottom + 6);
    }
  });
  const zero = toValue(0);
  ctx.strokeStyle = "#9d9db5";
  ctx.beginPath();
  if (horizontal) {
    ctx.moveTo(Math.round(zero) + 0.5, top);
    ctx.lineTo(Math.round(zero) + 0.5, bottom);
  } else {
    ctx.moveTo(left, Math.round(zero) + 0.5);
    ctx.lineTo(right, Math.round(zero) + 0.5);
  }
  ctx.stroke();

  if (chart.kind === "line") {
    series.forEach((one, s) => {
      ctx.strokeStyle = SERIES_COLORS[s % SERIES_COLORS.length];
      ctx.fillStyle = ctx.strokeStyle;
      ctx.lineWidth = 2;
      ctx.lineJoin = "round";
      ctx.beginPath();
      let pen = false;
      const points: [number, number][] = [];
      one.values.forEach((value, index) => {
        if (value === null) {
          pen = false;
          return;
        }
        const x = left + slot * (index + 0.5);
        const y = toValue(value);
        points.push([x, y]);
        if (pen) ctx.lineTo(x, y);
        else ctx.moveTo(x, y);
        pen = true;
      });
      ctx.stroke();
      for (const [x, y] of points) {
        ctx.beginPath();
        ctx.arc(x, y, 3, 0, Math.PI * 2);
        ctx.fill();
      }
    });
    return;
  }

  // Colunas ou barras agrupadas.
  const groupSize = slot * 0.72;
  const each = groupSize / series.length;
  series.forEach((one, s) => {
    ctx.fillStyle = SERIES_COLORS[s % SERIES_COLORS.length];
    one.values.forEach((value, index) => {
      if (value === null) return;
      const offset = (horizontal ? top : left) + slot * index + (slot - groupSize) / 2 + each * s;
      const from = toValue(0);
      const to = toValue(value);
      if (horizontal) ctx.fillRect(Math.min(from, to), offset, Math.max(1, Math.abs(to - from)), Math.max(1, each - 1));
      else ctx.fillRect(offset, Math.min(from, to), Math.max(1, each - 1), Math.max(1, Math.abs(to - from)));
    });
  });
}

/** Pizza: a primeira série, uma fatia por categoria. */
function drawPie(ctx: CanvasRenderingContext2D, chart: ChartInfo, box: Box) {
  const values = (chart.data.series[0]?.values ?? []).map((value) => (value !== null && value > 0 ? value : 0));
  const total = values.reduce((sum, value) => sum + value, 0);
  const radius = Math.min(box.width, box.height) / 2 - 10;
  if (total <= 0 || radius < 10) return;
  const cx = box.left + box.width / 2;
  const cy = box.top + box.height / 2;
  let angle = -Math.PI / 2;
  ctx.font = `bold 10px ${FONT}`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  values.forEach((value, index) => {
    if (value === 0) return;
    const sweep = (value / total) * Math.PI * 2;
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.arc(cx, cy, radius, angle, angle + sweep);
    ctx.closePath();
    ctx.fillStyle = SERIES_COLORS[index % SERIES_COLORS.length];
    ctx.fill();
    ctx.strokeStyle = "#fff";
    ctx.lineWidth = 1.5;
    ctx.stroke();
    if (sweep > 0.25) {
      const middle = angle + sweep / 2;
      ctx.fillStyle = "#fff";
      ctx.fillText(`${Math.round((value / total) * 100)}%`, cx + Math.cos(middle) * radius * 0.65, cy + Math.sin(middle) * radius * 0.65);
    }
    angle += sweep;
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
  const { series, categories } = chart.data;
  const pie = chart.kind === "pie";
  const names = pie ? categories : series.map((one) => one.name);
  const colors = SERIES_COLORS;
  const showLegend = pie || series.length > 1;
  const legendHeight = showLegend ? 24 : 0;
  const plot: Box = { left: box.left, top, width: box.width, height: box.top + box.height - top - legendHeight };
  if (plot.width >= 80 && plot.height >= 60) {
    if (pie) drawPie(ctx, chart, plot);
    else drawAxes(ctx, chart, plot);
    if (showLegend) legend(ctx, names, colors, { left: box.left, top: box.top + box.height - legendHeight, width: box.width, height: legendHeight });
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
