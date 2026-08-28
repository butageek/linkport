import type { Outcome } from "@/lib/types";
import { Badge } from "@/components/ui/badge";

/** `browserNames` maps browser ids to display names; ids without an entry
 * (e.g. a browser deleted after the event was logged) fall back to the id. */
export function OutcomeBadge({
  outcome,
  browserNames = {},
}: {
  outcome: Outcome;
  browserNames?: Record<string, string>;
}) {
  const name = (id: string) => browserNames[id] ?? id;
  switch (outcome.type) {
    // Both browser outcomes share one neutral style — the browser itself
    // is not more or less important because a rule or the default picked it.
    case "rule_matched":
      return (
        <Badge variant="secondary">
          {name(outcome.target)}
          {outcome.incognito ? " (private)" : ""}
        </Badge>
      );
    case "default":
      return <Badge variant="secondary">{name(outcome.target)} (default)</Badge>;
    case "blocked":
      return <Badge variant="destructive">blocked</Badge>;
    case "no_match":
      return <Badge variant="outline">no match</Badge>;
  }
}
