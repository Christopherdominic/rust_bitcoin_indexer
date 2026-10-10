"use client";

import { Moon, Sun } from "lucide-react";
import { useEffect, useSyncExternalStore } from "react";

type Theme = "light" | "dark";

const STORAGE_KEY = "theme";
const root = () => document.documentElement;

/** The theme currently applied to <html> (set before paint by app/layout.tsx). */
function subscribe(onChange: () => void) {
  const observer = new MutationObserver(onChange);
  observer.observe(root(), { attributes: true, attributeFilter: ["data-theme"] });
  return () => observer.disconnect();
}

const getTheme = (): Theme => (root().dataset.theme === "light" ? "light" : "dark");
// Unknown on the server; the button renders without an icon until hydrated.
const getServerTheme = (): Theme | null => null;

function savedTheme(): Theme | null {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return value === "light" || value === "dark" ? value : null;
  } catch {
    return null;
  }
}

/**
 * Switches between light and dark. Until the visitor picks one, the OS
 * preference is followed, including when it changes while the page is open.
 */
export function ThemeToggle() {
  const theme = useSyncExternalStore(subscribe, getTheme, getServerTheme);

  useEffect(() => {
    const query = matchMedia("(prefers-color-scheme: light)");
    const follow = () => {
      if (!savedTheme()) root().dataset.theme = query.matches ? "light" : "dark";
    };
    query.addEventListener("change", follow);
    return () => query.removeEventListener("change", follow);
  }, []);

  const next: Theme = theme === "light" ? "dark" : "light";

  const toggle = () => {
    root().dataset.theme = next;
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Storage blocked: the choice still applies for this page view.
    }
  };

  return (
    <button
      type="button"
      onClick={toggle}
      className="flex size-8 items-center justify-center rounded-md border border-line text-muted transition-colors hover:border-line-strong hover:bg-raised hover:text-fg"
      aria-label={theme ? `Switch to ${next} theme` : "Switch theme"}
      title={theme ? `Switch to ${next} theme` : undefined}
    >
      {theme === "light" ? <Moon className="size-4" /> : theme === "dark" ? <Sun className="size-4" /> : null}
    </button>
  );
}
