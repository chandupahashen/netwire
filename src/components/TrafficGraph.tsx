import { useEffect, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

interface Props {
  down: number[];
  up: number[];
  labels: number[];
  colors: { down: string; up: string; grid: string; text: string };
  height?: number;
  fill?: boolean;
}

/** Live throughput graph (uPlot — handles 60fps rolling data). */
export default function TrafficGraph({ down, up, labels, colors, height = 220, fill = false }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);

  useEffect(() => {
    if (!hostRef.current) return;
    const opts: uPlot.Options = {
      width: Math.max(320, hostRef.current.clientWidth),
      height: fill ? Math.max(240, hostRef.current.clientHeight) : height,
      scales: { x: { time: true }, y: { range: (_u, _min, max) => [0, Math.max(max * 1.15, 1024)] } },
      axes: [
        { stroke: colors.text, grid: { stroke: colors.grid } },
        {
          stroke: colors.text,
          grid: { stroke: colors.grid },
          values: (_u, vals) =>
            vals.map((v) => (v >= 1e6 ? `${(v / 1e6).toFixed(1)}M` : v >= 1e3 ? `${(v / 1e3).toFixed(0)}K` : `${v}`)),
        },
      ],
      series: [
        {},
        { label: "Down B/s", paths: uPlot.paths.spline?.(), stroke: colors.down, fill: colors.down + "2e", width: 2 },
        { label: "Up B/s", paths: uPlot.paths.spline?.(), stroke: colors.up, fill: colors.up + "1f", width: 2 },
      ],
      legend: { show: true },
      cursor: { show: true },
    };
    const plot = new uPlot(opts, [labels, down, up], hostRef.current);
    plotRef.current = plot;
    const resize = () => {
      const host = hostRef.current;
      if (!host) return;
      plot.setSize({
        width: Math.max(320, host.clientWidth),
        height: fill ? Math.max(240, host.clientHeight) : height,
      });
    };
    const observer = new ResizeObserver(resize);
    observer.observe(hostRef.current);
    const onResize = () => resize();
    window.addEventListener("resize", onResize);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", onResize);
      plot.destroy();
      plotRef.current = null;
    };
    // Rebuild on theme/accent change (cheap, infrequent).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [colors, fill, height]);

  useEffect(() => {
    plotRef.current?.setData([labels, down, up]);
  }, [down, up, labels]);

  return <div ref={hostRef} className={fill ? "traffic-graph-host fill" : "traffic-graph-host"} style={{ width: "100%", height: fill ? "100%" : undefined }} />;
}
