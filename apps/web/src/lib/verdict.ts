import type { Verdict } from "./api";

export interface VerdictMeta {
  /** Shape carries the meaning when color cannot (W5.1). */
  glyph: string;
  label: string;
  /** One plain sentence, shown in the legend and as a tooltip. */
  meaning: string;
}

export const VERDICTS: Record<Verdict, VerdictMeta> = {
  PROVEN: {
    glyph: "●",
    label: "Proven",
    meaning:
      "Captured before the date, and the same content was captured again after it. It did not change across the date.",
  },
  LIKELY: {
    glyph: "◐",
    label: "Likely",
    meaning: "Captured before the date, and nothing later contradicts it.",
  },
  UNCERTAIN: {
    glyph: "◇",
    label: "Uncertain",
    meaning:
      "Captures on both sides of the date differ. It changed in between, so the version on the day cannot be shown.",
  },
  ANACHRONISM: {
    glyph: "▲",
    label: "Anachronism",
    meaning:
      "The only capture is from after the date. The file may not have existed, or looked different, on the day.",
  },
  MISSING: {
    glyph: "✕",
    label: "Missing",
    meaning: "The archive holds no usable copy.",
  },
  UNKNOWN: {
    glyph: "?",
    label: "Unknown",
    meaning: "The archive could not be asked. This is not the same as missing.",
  },
};

export const VERDICT_ORDER: Verdict[] = [
  "PROVEN",
  "LIKELY",
  "UNCERTAIN",
  "ANACHRONISM",
  "MISSING",
  "UNKNOWN",
];
