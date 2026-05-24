import { useEffect, useRef, useState, useCallback } from "react";
import "./App.css";

const randomSeed = () => Math.floor(Math.random() * 1_000_000);

// ---- Layout descriptor types from WASM describe_layout ----
type LayoutDesc =
  | { type: "horizontal_stripes"; bands: number; wideMiddle: boolean; colors: string[] }
  | { type: "vertical_stripes"; bands: number; wideMiddle: boolean; colors: string[] }
  | { type: "nordic_cross"; thicknessFrac: number; colors: string[] }
  | { type: "canton"; colors: string[] }
  | { type: "rising_sun"; rays: number; cxFrac: number; cyFrac: number; colors: string[] }
  | { type: "nordic_star"; thicknessFrac: number; starScale: number; colors: string[] }
  | { type: "chevron"; colors: string[] };

// ---- SVG builder ----
function buildFlagSvg(layout: LayoutDesc, w: number, h: number): string {
  const lines: string[] = [
    `<?xml version="1.0" encoding="UTF-8"?>`,
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" shape-rendering="crispEdges">`,
  ];

  switch (layout.type) {
    case "horizontal_stripes": {
      const n = layout.bands;
      if (layout.wideMiddle && n === 3) {
        const splits = [0, 0.25, 0.75, 1.0];
        for (let i = 0; i < 3; i++) {
          const y = Math.round(splits[i] * h);
          const bh = Math.round(splits[i + 1] * h) - y;
          lines.push(`  <rect x="0" y="${y}" width="${w}" height="${bh}" fill="${layout.colors[i]}"/>`);
        }
      } else {
        for (let i = 0; i < n; i++) {
          const y = Math.round((i * h) / n);
          const bh = Math.round(((i + 1) * h) / n) - y;
          lines.push(`  <rect x="0" y="${y}" width="${w}" height="${bh}" fill="${layout.colors[i]}"/>`);
        }
      }
      break;
    }
    case "vertical_stripes": {
      const n = layout.bands;
      if (layout.wideMiddle) {
        const splits = [0, 0.25, 0.75, 1.0];
        for (let i = 0; i < 3; i++) {
          const x = Math.round(splits[i] * w);
          const bw = Math.round(splits[i + 1] * w) - x;
          lines.push(`  <rect x="${x}" y="0" width="${bw}" height="${h}" fill="${layout.colors[i]}"/>`);
        }
      } else {
        for (let i = 0; i < n; i++) {
          const x = Math.round((i * w) / n);
          const bw = Math.round(((i + 1) * w) / n) - x;
          lines.push(`  <rect x="${x}" y="0" width="${bw}" height="${h}" fill="${layout.colors[i]}"/>`);
        }
      }
      break;
    }
    case "nordic_cross": {
      const [bg, cross] = layout.colors;
      const crossX = w * (5 / 12);
      const crossY = h * 0.5;
      const t = Math.min(w, h) * layout.thicknessFrac;
      lines.push(
        `  <rect x="0" y="0" width="${w}" height="${h}" fill="${bg}"/>`,
        `  <rect x="${(crossX - t).toFixed(1)}" y="0" width="${(t * 2).toFixed(1)}" height="${h}" fill="${cross}"/>`,
        `  <rect x="0" y="${(crossY - t).toFixed(1)}" width="${w}" height="${(t * 2).toFixed(1)}" fill="${cross}"/>`
      );
      break;
    }
    case "canton": {
      const [field, canton] = layout.colors;
      lines.push(
        `  <rect x="0" y="0" width="${w}" height="${h}" fill="${field}"/>`,
        `  <rect x="0" y="0" width="${Math.floor(w * 0.4)}" height="${Math.floor(h * 0.5)}" fill="${canton}"/>`
      );
      break;
    }
    case "rising_sun": {
      const [c0, c1] = layout.colors;
      const { rays, cxFrac, cyFrac } = layout;
      const cx = w * cxFrac;
      const cy = h * cyFrac;
      lines.push(`  <rect x="0" y="0" width="${w}" height="${h}" fill="${c0}"/>`);
      const maxR = Math.sqrt(w * w + h * h);
      const sector = (2 * Math.PI) / rays;
      for (let i = 0; i < rays; i += 2) {
        const a0 = i * sector;
        const a1 = (i + 1) * sector;
        const x0 = cx + Math.cos(a0) * maxR;
        const y0 = cy + Math.sin(a0) * maxR;
        const x1 = cx + Math.cos(a1) * maxR;
        const y1 = cy + Math.sin(a1) * maxR;
        lines.push(`  <polygon points="${cx.toFixed(1)},${cy.toFixed(1)} ${x0.toFixed(1)},${y0.toFixed(1)} ${x1.toFixed(1)},${y1.toFixed(1)}" fill="${c1}"/>`);
      }
      break;
    }
    case "nordic_star": {
      const [bg, cross, star] = layout.colors;
      const crossX = w * (5 / 12);
      const crossY = h * 0.5;
      const t = Math.min(w, h) * layout.thicknessFrac;
      lines.push(
        `  <rect x="0" y="0" width="${w}" height="${h}" fill="${bg}"/>`,
        `  <rect x="${(crossX - t).toFixed(1)}" y="0" width="${(t * 2).toFixed(1)}" height="${h}" fill="${cross}"/>`,
        `  <rect x="0" y="${(crossY - t).toFixed(1)}" width="${w}" height="${(t * 2).toFixed(1)}" fill="${cross}"/>`
      );
      const r = Math.min(w, h) * layout.starScale;
      const pts: string[] = [];
      for (let i = 0; i < 5; i++) {
        const outerA = -Math.PI / 2 + (i * 2 * Math.PI) / 5;
        const innerA = outerA + Math.PI / 5;
        pts.push(`${(crossX + Math.cos(outerA) * r).toFixed(1)},${(crossY + Math.sin(outerA) * r).toFixed(1)}`);
        pts.push(`${(crossX + Math.cos(innerA) * r * 0.38).toFixed(1)},${(crossY + Math.sin(innerA) * r * 0.38).toFixed(1)}`);
      }
      lines.push(`  <polygon points="${pts.join(" ")}" fill="${star}"/>`);
      break;
    }
    case "chevron": {
      const [field, chevron] = layout.colors;
      const apexX = w * 0.4;
      lines.push(
        `  <rect x="0" y="0" width="${w}" height="${h}" fill="${field}"/>`,
        `  <polygon points="0,0 ${apexX},${h / 2} 0,${h}" fill="${chevron}"/>`
      );
      break;
    }
  }

  lines.push(`</svg>`);
  return lines.join("\n");
}

function triggerDownload(href: string, filename: string) {
  const a = document.createElement("a");
  a.href = href;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

function App() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const workerRef = useRef<Worker | null>(null);

  const [width, setWidth] = useState(512);
  const [height, setHeight] = useState(512);
  const [seed, setSeed] = useState<number>(() => randomSeed());
  const [filter, setFilter] = useState("none");
  const [enableFabric, setEnableFabric] = useState(true);
  const [layout, setLayout] = useState<LayoutDesc | null>(null);
  const [processing, setProcessing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pixelsRef = useRef<Uint8Array | null>(null);

  useEffect(() => {
    const worker = new Worker(
      new URL("./workers/flag.worker.ts", import.meta.url),
      { type: "module" }
    );
    worker.onmessage = (e: MessageEvent) => {
      const { action, pixels, width: w, height: h, layout: desc, error: err } = e.data;
      if (action === "ERROR") { setError(err); setProcessing(false); return; }
      setError(null);
      setProcessing(false);
      const canvas = canvasRef.current;
      if (!canvas || !pixels) return;
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      ctx.putImageData(new ImageData(new Uint8ClampedArray(pixels), w, h), 0, 0);
      if (action === "GENERATE") {
        pixelsRef.current = new Uint8Array(pixels);
        if (desc) setLayout(desc as LayoutDesc);
      }
    };
    workerRef.current = worker;
    return () => { worker.terminate(); workerRef.current = null; };
  }, []);

  useEffect(() => {
    const worker = workerRef.current;
    if (!worker) return;
    setProcessing(true);
    setFilter("none");
    worker.postMessage({ action: "GENERATE", width, height, seed, enableFabric });
  }, [seed, width, height, enableFabric]);

  const generate = useCallback(() => { setSeed(randomSeed()); }, []);

  const applyFilter = useCallback((filterType: string) => {
    setFilter(filterType);
    if (filterType === "none" || !pixelsRef.current || !workerRef.current) {
      if (filterType === "none" && pixelsRef.current && canvasRef.current) {
        const canvas = canvasRef.current;
        const ctx = canvas.getContext("2d");
        if (ctx) ctx.putImageData(new ImageData(new Uint8ClampedArray(pixelsRef.current), canvas.width, canvas.height), 0, 0);
      }
      return;
    }
    setProcessing(true);
    const copy = new Uint8Array(pixelsRef.current);
    workerRef.current.postMessage({ action: "FILTER", pixels: copy, filterType, width, height }, [copy.buffer]);
  }, [width, height]);

  const exportPng = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    triggerDownload(canvas.toDataURL("image/png"), `flag-${seed}.png`);
  }, [seed]);

  const exportSvg = useCallback(() => {
    if (!layout) return;
    const svg = buildFlagSvg(layout, width, height);
    const blob = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    triggerDownload(url, `flag-${seed}-flat.svg`);
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }, [layout, seed, width, height]);

  return (
    <div className="app">
      <h1>Flag Generator</h1>
      <div className="controls">
        <label>
          Width: {width}
          <input type="range" min={64} max={1024} step={64} value={width} onChange={(e) => setWidth(Number(e.target.value))} />
        </label>
        <label>
          Height: {height}
          <input type="range" min={64} max={1024} step={64} value={height} onChange={(e) => setHeight(Number(e.target.value))} />
        </label>
        <label>
          Seed: {seed}
          <input type="range" min={0} max={1_000_000} step={1} value={seed} onChange={(e) => setSeed(Number(e.target.value))} />
        </label>
        <label>
          Filter:
          <select value={filter} onChange={(e) => applyFilter(e.target.value)}>
            <option value="none">None</option>
            <option value="grayscale">Grayscale</option>
            <option value="invert">Invert</option>
            <option value="vignette">Vignette</option>
          </select>
        </label>
        <label className="toggle">
          <input type="checkbox" checked={enableFabric} onChange={(e) => setEnableFabric(e.target.checked)} />
          Enable 3D Fabric Effect
        </label>
      </div>
      <div className="actions">
        <button onClick={generate} disabled={processing}>
          {processing ? "Processing…" : "🎲 Generate"}
        </button>
        <button onClick={exportPng} disabled={processing} className="export">⬇️ Export PNG</button>
        <button onClick={exportSvg} disabled={processing || !layout} className="export">⬇️ Export SVG (Flat)</button>
      </div>
      {error && <p className="error">{error}</p>}
      <canvas ref={canvasRef} className="canvas" />
    </div>
  );
}

export default App;
