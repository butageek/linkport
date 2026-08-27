import type { Outcome } from "@/lib/types";
import { Badge } from "@/components/ui/badge";

export function OutcomeBadge({ outcome }: { outcome: Outcome }) {
  switch (outcome.type) {
    case "rule_matched":
      return (
        <Badge variant="default">
          {outcome.target}
          {outcome.incognito ? " (private)" : ""}
        </Badge>
      );
    case "default":
      return <Badge variant="secondary">{outcome.target} (default)</Badge>;
    case "blocked":
      return <Badge variant="destructive">blocked</Badge>;
    case "no_match":
      return <Badge variant="outline">no match</Badge>;
  }
}
