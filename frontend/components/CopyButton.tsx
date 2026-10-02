"use client";

import { Check, Copy } from "lucide-react";
import { useEffect, useState } from "react";

async function writeClipboard(text: string) {
  if (navigator.clipboard && window.isSecureContext) {
    await navigator.clipboard.writeText(text);
    return;
  }
  // Fallback for non-secure origins (e.g. LAN IP during a demo).
  const el = document.createElement("textarea");
  el.value = text;
  el.setAttribute("readonly", "");
  el.style.position = "fixed";
  el.style.opacity = "0";
  document.body.appendChild(el);
  el.select();
  document.execCommand("copy");
  el.remove();
}

export function CopyButton({ value, label = "Copy" }: { value: string; label?: string }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 1400);
    return () => clearTimeout(t);
  }, [copied]);

  return (
    <button
      type="button"
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        writeClipboard(value).then(() => setCopied(true), () => undefined);
      }}
      title={copied ? "Copied" : `${label}: ${value}`}
      aria-label={copied ? "Copied" : label}
      className="inline-flex size-6 shrink-0 items-center justify-center rounded text-faint transition-colors hover:bg-raised hover:text-fg focus-visible:text-btc"
    >
      {copied ? <Check className="size-3.5 text-ok" /> : <Copy className="size-3.5" />}
    </button>
  );
}
