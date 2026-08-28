"use client";

import { useId } from "react";

// The Linkport mark: interlocked chain rings (over-under weave — each
// ring's stroke breaks where the other passes over it) on an indigo
// rounded square with a subtle diagonal gradient. Mirrors
// resources/icon.ico / scripts/gen_icon.py — keep the geometry in sync.
export function Logo({ className }: { className?: string }) {
  const uid = useId().replace(/[^a-zA-Z0-9]/g, "");
  const grad = `logo-grad-${uid}`;
  const cutTop = `logo-cut-top-${uid}`;
  const cutBottom = `logo-cut-bottom-${uid}`;
  return (
    <svg viewBox="0 0 32 32" className={className} aria-hidden="true">
      <defs>
        <linearGradient id={grad} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#6366f1" />
          <stop offset="1" stopColor="#4338ca" />
        </linearGradient>
        <mask id={cutTop}>
          <rect width="32" height="32" fill="#fff" />
          <circle cx="16" cy="12.6" r="2.1" fill="#000" />
        </mask>
        <mask id={cutBottom}>
          <rect width="32" height="32" fill="#fff" />
          <circle cx="16" cy="19.4" r="2.1" fill="#000" />
        </mask>
      </defs>
      <rect x="1.5" y="1.5" width="29" height="29" rx="7" fill={`url(#${grad})`} />
      <g fill="none" stroke="#fff" strokeWidth="2">
        <circle cx="20" cy="16" r="4.25" mask={`url(#${cutTop})`} />
        <circle cx="12" cy="16" r="4.25" mask={`url(#${cutBottom})`} />
      </g>
    </svg>
  );
}
