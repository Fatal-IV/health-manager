import { CheckCircle, Question, WarningCircle, XCircle } from "@phosphor-icons/react";
import type { Status } from "../api";

/** Status glyph; colour comes from the `s-<status>` class. Decorative, the text label carries the meaning. */
export function StatusIcon({ status, size = 20 }: { status: Status; size?: number }) {
  const props = { size, weight: "fill" as const, "aria-hidden": true, className: `s-${status}` };
  if (status === "good") return <CheckCircle {...props} />;
  if (status === "warning") return <WarningCircle {...props} />;
  if (status === "critical") return <XCircle {...props} />;
  return <Question {...props} />;
}
