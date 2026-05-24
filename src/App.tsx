import { useEffect, useRef, useState, useCallback } from "react";
import "./App.css";

const randomSeed = () => Math.floor(Math.random() * 1_000_000);

// ---- Layout descriptor shipped from the WASM worker ------------------
// Mirrors the JSON shape produced by `describe_layout` in lib.rs.
type LayoutDesc =
  | { type: "horizontal_stripes"; bands: number; colors: string[] }
  | { type: "vertical_stripes"; bands: number; colors: string[] }
  | { type: "nordic_cross"; colors: string[] }
  | { type: "canton"; colors: string[] };

// ---- Pure SVG builder ------------------------------------------------
// Reproduces the geometric proportions used by `zone_color` in lib.rs
// so the exported vector matches the rasterised flat output exactly.
function buildFlagSvg(layout: LayoutDesc, width: number, height: number): string {
  const lines: string[] = [
    `<?xml version="1.0" encoding="UTF-8"?>`,
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" shape-rendering="crispEdges">`,
  ];

  switch (layout.type) {
    case "horizontal_stripes": {
      const n = layout.bands;
      // Use exact band heights to avoid sub-pixel seams. The last band
      // takes whatever remains so the total always sums to `height`.
      for (let i = 0; i < n; i++) {
        const y = Math.round((i * height) / n);
        const yNext = Math.round(((i + 1) * height) / n);
        const bandH = yNext - y;
        lines.push(
          `  <rect x="0" y="${y}" width="${width}" height="${bandH}" fill="${layout.colors[i]}"/>`
        );
      }
      break;
    }
    case "vertical_stripes": {
      const n = layout.bands;
      for (let i = 0; i < n; i++) {
        const x = Math.round((i * width) / n);
        const xNext = Math.round(((i + 1) * width) / n);
        const bandW = xNext - x;
        lines.push(
          `  <rect x="${x}" y="0" width="${bandW}" height="${height}" fill="${layout.colors[i]}"/>`
        );
      }
      break;
    }
    case "nordic_cross": {
      const [bg, cross] = layout.colors;
      // Match lib.rs: cross_x = (w*5)/12, cross_y = h/2,
      // thickness = min(w,h)/7, bars span ±thickness.
      const crossX = Math.floor((width * 5) / 12);
      const crossY = Math.floor(height / 2);
      const thickness = Math.floor(Math.min(width, height) / 7);
      const barW = thickness * 2;
      lines.push(
        `  <rect x="0" y="0" width="${width}" height="${height}" fill="${bg}"/>`,
        `  <rect x="${crossX - thickness}" y="0" width="${barW}" height="${height}" fill="${cross}"/>`,
        `  <rect x="0" y="${crossY - thickness}" width="${width}" height="${barW}" fill="${cross}"/>`
      );
      break;
    }
    case "canton": {
      const [field, canton] = layout.colors;
      const cantonW = Math.floor((width * 2) / 5);
      const cantonH = Math.floor(height / 2);
      lines.push(
        `  <rect x="0" y="0" width="${width}" height="${height}" fill="${field}"/>`,
        `  <rect x="0" y="0" width="${cantonW}" height="${cantonH}" fill="${canton}"/>`
      );
      break;
    }
  }

  lines.push(`</svg>`);
  return lines.join("\n");
}

// ---- Trigger a browser download for a given Blob/URL pair -----------
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

  // Holds the most recent unfiltered pixels so filters can be re-applied
  // to the original output instead of compounding.
  const pixelsRef = useRef<Uint8Array | null>(null);

  // ---- Worker lifecycle (created once, terminated on unmount) ----
  useEffect(() => {
    const worker = new Worker(
      new URL("./workers/flag.worker.ts", import.meta.url),
      { type: "module" }
    );

    worker.onmessage = (e: MessageEvent) => {
      const {
        action,
        pixels,
        width: w,
        height: h,
        layout: layoutDesc,
        error: err,
      } = e.data;

      if (action === "ERROR") {
        setError(err);
        setProcessing(false);
        return;
      }

      setError(null);
      setProcessing(false);

      const canvas = canvasRef.current;
      if (!canvas || !pixels) return;

      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;

      const imageData = new ImageData(new Uint8ClampedArray(pixels), w, h);
      ctx.putImageData(imageData, 0, 0);

      if (action === "GENERATE") {
        // Snapshot for filter re-application.
        pixelsRef.current = new Uint8Array(pixels);
        if (layoutDesc) setLayout(layoutDesc as LayoutDesc);
      }
    };

    workerRef.current = worker;
    return () => {
      worker.terminate();
      workerRef.current = null;
    };
  }, []);

  // ---- Auto-regenerate when seed / dimensions / fabric toggle change ----
  useEffect(() => {
    const worker = workerRef.current;
    if (!worker) return;
    setProcessing(true);
    setFilter("none");
    worker.postMessage({
      action: "GENERATE",
      width,
      height,
      seed,
      enableFabric,
    });
  }, [seed, width, height, enableFabric]);

  // ---- "Generate" button: randomize seed → effect above repaints ----
  const generate = useCallback(() => {
    setSeed(randomSeed());
  }, []);

  const applyFilter = useCallback(
    (filterType: string) => {
      setFilter(filterType);
      if (filterType === "none" || !pixelsRef.current || !workerRef.current) {
        // Re-render the unfiltered snapshot if we have one.
        if (filterType === "none" && pixelsRef.current && canvasRef.current) {
          const canvas = canvasRef.current;
          const ctx = canvas.getContext("2d");
          if (ctx) {
            const imageData = new ImageData(
              new Uint8ClampedArray(pixelsRef.current),
              canvas.width,
              canvas.height
            );
            ctx.putImageData(imageData, 0, 0);
          }
        }
        return;
      }
      setProcessing(true);
      const copy = new Uint8Array(pixelsRef.current);
      workerRef.current.postMessage(
        { action: "FILTER", pixels: copy, filterType, width, height },
        [copy.buffer]
      );
    },
    [width, height]
  );

  // ---- Export: rasterised PNG straight from the canvas ----
  const exportPng = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    // toDataURL captures the current canvas state — including any
    // filter currently applied — so what-you-see is what-you-get.
    const dataUrl = canvas.toDataURL("image/png");
    triggerDownload(dataUrl, `flag-${seed}.png`);
  }, [seed]);

  // ---- Export: pure-vector SVG of the flat geometric layout ----
  const exportSvg = useCallback(() => {
    if (!layout) return;
    const svg = buildFlagSvg(layout, width, height);
    const blob = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    try {
      triggerDownload(url, `flag-${seed}-flat.svg`);
    } finally {
      // Defer revoke so Chrome reliably finishes the download.
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    }
  }, [layout, seed, width, height]);

  return (
    <div className="app">
      <h1>Flag Generator</h1>

      <div className="controls">
        <label>
          Width: {width}
          <input
            type="range"
            min={64}
            max={1024}
            step={64}
            value={width}
            onChange={(e) => setWidth(Number(e.target.value))}
          />
        </label>

        <label>
          Height: {height}
          <input
            type="range"
            min={64}
            max={1024}
            step={64}
            value={height}
            onChange={(e) => setHeight(Number(e.target.value))}
          />
        </label>

        <label>
          Seed: {seed}
          <input
            type="range"
            min={0}
            max={1_000_000}
            step={1}
            value={seed}
            onChange={(e) => setSeed(Number(e.target.value))}
          />
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
          <input
            type="checkbox"
            checked={enableFabric}
            onChange={(e) => setEnableFabric(e.target.checked)}
          />
          Enable 3D Fabric Effect
        </label>
      </div>

      <div className="actions">
        <button onClick={generate} disabled={processing}>
          {processing ? "Processing…" : "🎲 Generate"}
        </button>
        <button
          onClick={exportPng}
          disabled={processing}
          className="export"
          title="Save the current canvas as a PNG"
        >
          ⬇️ Export PNG
        </button>
        <button
          onClick={exportSvg}
          disabled={processing || !layout}
          className="export"
          title="Save the flat geometric layout as a pure-vector SVG"
        >
          ⬇️ Export SVG (Flat)
        </button>
      </div>

      {error && <p className="error">{error}</p>}

      <canvas ref={canvasRef} className="canvas" />
    </div>
  );
}

export default App;
