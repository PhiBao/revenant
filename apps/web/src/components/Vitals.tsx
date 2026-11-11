import type { Certificate } from "@/lib/api";
import { formatPercent } from "@/lib/api";

function Vital({
  name,
  value,
  gloss,
}: {
  name: string;
  value: number | null;
  gloss: string;
}) {
  return (
    <div className="vital">
      <div className="vital-name">{name}</div>
      <div className="vital-value" aria-label={`${name} ${formatPercent(value)} percent`}>
        {formatPercent(value)}
        <span className="vital-unit">%</span>
      </div>
      <p className="vital-gloss">{gloss}</p>
    </div>
  );
}

export function Vitals({ cert }: { cert: Certificate }) {
  const v = cert.vitals;
  return (
    <div className="vitals">
      <Vital
        name="Completeness"
        value={v.completeness_percent}
        gloss="Share of the files this page uses that the archive holds a usable copy of."
      />
      <Vital
        name="Fidelity"
        value={v.fidelity_percent}
        gloss="Share of those files known, or likely, to look the same on the day you chose."
      />
    </div>
  );
}
