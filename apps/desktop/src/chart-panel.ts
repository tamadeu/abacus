import { seriesColor } from "./chart-draw";
import type { ChartInfo, LegendPos, SeriesStyle, Stacking, Trend, TrendKind } from "./engine";

type Patch = (chart: ChartInfo) => ChartInfo | null;

interface Deps {
  /** Aplica uma mudança ao gráfico aberto no painel e grava. */
  change(patch: Patch): void;
}

const TREND_LABELS: [TrendKind | "", string][] = [
  ["", "Nenhuma"],
  ["linear", "Linear"],
  ["poly", "Polinomial"],
  ["exp", "Exponencial"],
  ["log", "Logarítmica"],
  ["moving", "Média móvel"],
];

const $ = <T extends HTMLElement>(selector: string) => document.querySelector<T>(selector)!;

/** Escreve num campo sem atrapalhar quem está digitando nele. */
function put(input: HTMLInputElement | HTMLSelectElement, value: string) {
  if (document.activeElement !== input) input.value = value;
}

const putCheck = (input: HTMLInputElement, value: boolean) => {
  input.checked = value;
};

/** `1,5` ou `1.5` como número; vazio é "automático". */
function parseLimit(text: string): number | null | undefined {
  const clean = text.trim().replace(",", ".");
  if (clean === "") return null;
  const value = Number(clean);
  return Number.isFinite(value) ? value : undefined;
}

const limitText = (value: number | null) => (value === null ? "" : String(value).replace(".", ","));

function emptyStyle(): SeriesStyle {
  return { color: null, line: false, secondary: false, trend: null };
}

/** Ajusta a série `index` mantendo as demais como estão. */
function withStyle(chart: ChartInfo, index: number, edit: (style: SeriesStyle) => SeriesStyle): ChartInfo {
  const styles = [...chart.styles];
  while (styles.length <= index) styles.push(emptyStyle());
  styles[index] = edit({ ...styles[index] });
  return { ...chart, styles };
}

export function setupChartOptions({ change }: Deps) {
  const stacking = $<HTMLSelectElement>("#chart-stacking");
  const legend = $<HTMLSelectElement>("#chart-legend");
  const labels = $<HTMLInputElement>("#chart-labels");
  const grid = $<HTMLInputElement>("#chart-grid");
  const smooth = $<HTMLInputElement>("#chart-smooth");
  const xTitle = $<HTMLInputElement>("#chart-xtitle");
  const yTitle = $<HTMLInputElement>("#chart-ytitle");
  const yMin = $<HTMLInputElement>("#chart-ymin");
  const yMax = $<HTMLInputElement>("#chart-ymax");
  const seriesBox = $("#chart-series-box");
  const seriesList = $("#chart-series");
  let signature = "";

  stacking.addEventListener("change", () => change((c) => ({ ...c, stacking: stacking.value as Stacking })));
  legend.addEventListener("change", () => change((c) => ({ ...c, legend: legend.value as LegendPos })));
  labels.addEventListener("change", () => change((c) => ({ ...c, labels: labels.checked })));
  grid.addEventListener("change", () => change((c) => ({ ...c, grid: grid.checked })));
  smooth.addEventListener("change", () => change((c) => ({ ...c, smooth: smooth.checked })));
  xTitle.addEventListener("change", () => change((c) => ({ ...c, xTitle: xTitle.value.trim() })));
  yTitle.addEventListener("change", () => change((c) => ({ ...c, yTitle: yTitle.value.trim() })));
  const limit = (input: HTMLInputElement, key: "yMin" | "yMax") =>
    input.addEventListener("change", () => {
      const value = parseLimit(input.value);
      input.classList.toggle("invalid", value === undefined);
      if (value === undefined) return;
      change((c) => ({ ...c, [key]: value }));
    });
  limit(yMin, "yMin");
  limit(yMax, "yMax");

  /** O que muda a estrutura da lista de séries (e a obriga a ser desenhada de novo). */
  const signatureOf = (chart: ChartInfo) =>
    JSON.stringify([
      chart.kind,
      chart.id,
      chart.data.series.map((s) => s.name),
      chart.data.series.map((_, i) => {
        const style = chart.styles[i];
        return [style?.trend?.kind ?? "", style?.trend?.equation, style?.trend?.r2];
      }),
    ]);

  function row(label: string, control: HTMLElement): HTMLLabelElement {
    const wrapper = document.createElement("label");
    wrapper.append(label, control);
    return wrapper;
  }

  function check(text: string, value: boolean, onChange: (checked: boolean) => void): HTMLLabelElement {
    const wrapper = document.createElement("label");
    wrapper.className = "check";
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = value;
    input.addEventListener("change", () => onChange(input.checked));
    wrapper.append(input, ` ${text}`);
    return wrapper;
  }

  function renderSeries(chart: ChartInfo) {
    seriesList.replaceChildren();
    chart.data.series.forEach((one, index) => {
      const style = chart.styles[index] ?? emptyStyle();
      const block = document.createElement("div");
      block.className = "series";

      const head = document.createElement("div");
      head.className = "series-head";
      const color = document.createElement("input");
      color.type = "color";
      color.value = seriesColor(chart, index).toLowerCase();
      color.title = "Cor da série";
      color.addEventListener("change", () => change((c) => withStyle(c, index, (s) => ({ ...s, color: color.value.toUpperCase() }))));
      const name = document.createElement("span");
      name.textContent = one.name;
      head.append(color, name);
      block.append(head);

      if (chart.kind === "column") {
        block.append(check("Mostrar como linha", style.line, (on) => change((c) => withStyle(c, index, (s) => ({ ...s, line: on })))));
      }
      if (chart.kind === "column" || chart.kind === "line" || chart.kind === "area") {
        block.append(
          check("Eixo secundário (direita)", style.secondary, (on) => change((c) => withStyle(c, index, (s) => ({ ...s, secondary: on })))),
        );
      }
      if (chart.kind !== "pie" && chart.kind !== "donut") {
        const select = document.createElement("select");
        for (const [value, text] of TREND_LABELS) select.add(new Option(text, value));
        select.value = style.trend?.kind ?? "";
        select.addEventListener("change", () =>
          change((c) =>
            withStyle(c, index, (s) => ({
              ...s,
              trend: select.value
                ? { kind: select.value as TrendKind, order: s.trend?.order ?? 2, period: s.trend?.period ?? 2, equation: s.trend?.equation ?? false, r2: s.trend?.r2 ?? false }
                : null,
            })),
          ),
        );
        block.append(row("Linha de tendência", select));
        const trend = style.trend;
        if (trend) {
          const extra = document.createElement("div");
          extra.className = "trend-extra";
          const edit = (patch: Partial<Trend>) => change((c) => withStyle(c, index, (s) => ({ ...s, trend: s.trend ? { ...s.trend, ...patch } : s.trend })));
          if (trend.kind === "poly" || trend.kind === "moving") {
            const number = document.createElement("input");
            number.type = "number";
            number.min = "2";
            number.max = trend.kind === "poly" ? "6" : "100";
            number.value = String(trend.kind === "poly" ? trend.order : trend.period);
            number.addEventListener("change", () => {
              const value = Math.round(Number(number.value));
              if (Number.isFinite(value)) edit(trend.kind === "poly" ? { order: value } : { period: value });
            });
            extra.append(row(trend.kind === "poly" ? "Grau" : "Pontos", number));
          }
          if (trend.kind !== "moving") {
            extra.append(check("Equação", trend.equation, (on) => edit({ equation: on })), check("R²", trend.r2, (on) => edit({ r2: on })));
          }
          block.append(extra);
        }
      }
      seriesList.append(block);
    });
  }

  /** Mostra no painel o estado do gráfico. */
  function fill(chart: ChartInfo) {
    put(stacking, chart.stacking);
    put(legend, chart.legend);
    putCheck(labels, chart.labels);
    putCheck(grid, chart.grid);
    putCheck(smooth, chart.smooth);
    put(xTitle, chart.xTitle);
    put(yTitle, chart.yTitle);
    put(yMin, limitText(chart.yMin));
    put(yMax, limitText(chart.yMax));
    const round = chart.kind === "pie" || chart.kind === "donut";
    stacking.disabled = round || chart.kind === "scatter";
    smooth.disabled = chart.kind !== "line" && chart.kind !== "scatter";
    grid.disabled = round;
    $("#chart-axes").hidden = round;
    seriesBox.hidden = round;
    const next = signatureOf(chart);
    if (next !== signature || seriesList.childElementCount !== chart.data.series.length) {
      signature = next;
      renderSeries(chart);
    }
  }

  return { fill, forget: () => (signature = "") };
}
