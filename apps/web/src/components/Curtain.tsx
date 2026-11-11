"use client";

import { useEffect, useId, useRef, useState } from "react";

/** Divider positions the intro sweep passes through, with the seconds each leg takes (W10.6). */
const SWEEP: [number, number][] = [
  [100, 1.2],
  [100, 0.6],
  [0, 2.0],
  [0, 0.6],
  [50, 1.0],
];

interface Props {
  /** The "before" restore, drawn on the left of the divider. */
  nearestUrl: string;
  /** The restore that keeps only what can be shown as it was on the day. */
  dayUrl: string;
  dayLabel: string;
  /** What the left side is, for example "What archive playback gives". */
  leftLabel?: string;
  /** Short name of the left side for the toggle button. */
  leftShort?: string;
}

/**
 * Two restorations of the same page over one viewport. The nearest-capture restore sits on the
 * left of the divider, the as-it-was restore on the right. Both previews are sandboxed (W2.3).
 */
export function Curtain({
  nearestUrl,
  dayUrl,
  dayLabel,
  leftLabel = "What archive playback gives",
  leftShort = "Archive playback",
}: Props) {
  const [pos, setPos] = useState(50);
  const id = useId();
  const figure = useRef<HTMLElement>(null);
  const stop = useRef(false);

  // One intro sweep when the view is first mostly visible, unless motion is reduced (W10.6).
  useEffect(() => {
    const el = figure.current;
    if (!el || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    let frame = 0;
    let ran = false;
    const run = () => {
      let from = 50;
      let leg = 0;
      let legStart = performance.now();
      const step = (now: number) => {
        if (stop.current) return;
        const [to, secs] = SWEEP[leg]!;
        const t = Math.min(1, (now - legStart) / (secs * 1000));
        const eased = t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2;
        setPos(Math.round(from + (to - from) * eased));
        if (t >= 1) {
          from = to;
          leg += 1;
          legStart = now;
          if (leg >= SWEEP.length) return;
        }
        frame = requestAnimationFrame(step);
      };
      frame = requestAnimationFrame(step);
    };
    const seen = new IntersectionObserver(
      (entries) => {
        if (!ran && entries.some((e) => e.isIntersecting)) {
          ran = true;
          seen.disconnect();
          run();
        }
      },
      { threshold: 0.6 },
    );
    seen.observe(el);
    return () => {
      seen.disconnect();
      cancelAnimationFrame(frame);
    };
  }, []);

  const touch = () => {
    stop.current = true;
  };

  return (
    <figure className="curtain" ref={figure}>
      <div className="curtain-labels" aria-hidden="true">
        <span>{leftLabel}</span>
        <span>{dayLabel}</span>
      </div>

      <div className="curtain-view">
        <iframe
          className="curtain-frame curtain-frame-day"
          src={dayUrl}
          title={`Restoration as it was on ${dayLabel}`}
          sandbox=""
        />
        <iframe
          className="curtain-frame curtain-frame-nearest"
          src={nearestUrl}
          title={leftLabel}
          sandbox=""
          style={{ clipPath: `inset(0 ${100 - pos}% 0 0)` }}
        />
        <div className="curtain-line" style={{ left: `${pos}%` }} aria-hidden="true" />
      </div>

      <div className="curtain-controls">
        <label htmlFor={id} className="curtain-slider-label">
          Drag to compare
        </label>
        <input
          id={id}
          className="curtain-slider"
          type="range"
          min={0}
          max={100}
          value={pos}
          onPointerDown={touch}
          onKeyDown={touch}
          onChange={(e) => {
            touch();
            setPos(Number(e.target.value));
          }}
          aria-valuetext={`${pos} percent of the left side shown`}
        />
        <div className="curtain-toggle" role="group" aria-label="Choose a restoration">
          <button type="button" aria-pressed={pos === 100} onClick={() => {
              touch();
              setPos(100);
            }}>
            {leftShort}
          </button>
          <button type="button" aria-pressed={pos === 0} onClick={() => {
              touch();
              setPos(0);
            }}>
            As it was
          </button>
        </div>
      </div>
    </figure>
  );
}
