import { useRef } from "react";
import type { PointerEvent } from "react";

interface Props {
  label: string;
  value: number;
  min: number;
  max: number;
  reverse?: boolean;
  onChange: (value: number) => void;
}

export default function Splitter({ label, value, min, max, reverse = false, onChange }: Props) {
  const drag = useRef<{ x: number; width: number } | null>(null);
  function resize(delta: number) { onChange(Math.min(max, Math.max(min, value + delta))); }
  function move(event: PointerEvent<HTMLDivElement>) {
    if (!drag.current) return;
    const delta = (event.clientX - drag.current.x) * (reverse ? -1 : 1);
    onChange(Math.min(max, Math.max(min, drag.current.width + delta)));
  }
  return <div className="splitter" role="separator" aria-label={label} aria-orientation="vertical" aria-valuenow={value} aria-valuemin={min} aria-valuemax={max} tabIndex={0}
    onPointerDown={(event) => { drag.current = { x: event.clientX, width: value }; event.currentTarget.setPointerCapture(event.pointerId); event.preventDefault(); }}
    onPointerMove={move} onPointerUp={() => { drag.current = null; }} onPointerCancel={() => { drag.current = null; }} onLostPointerCapture={() => { drag.current = null; }}
    onKeyDown={(event) => { if (event.key === "ArrowLeft" || event.key === "ArrowRight") { event.preventDefault(); resize((event.key === "ArrowRight" ? 16 : -16) * (reverse ? -1 : 1)); } }}
  />;
}
