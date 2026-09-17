"use client";

import { useEffect, useState } from "react";

type Theme = "light" | "dark";

function preferredTheme(): Theme {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function ThemeToggle() {
  const [theme, setTheme] = useState<Theme>("light");

  useEffect(() => {
    const selected = window.localStorage.getItem("mads-docs-theme");
    const nextTheme: Theme = selected === "dark" || selected === "light" ? selected : preferredTheme();
    document.documentElement.dataset.theme = nextTheme;
    setTheme(nextTheme);
  }, []);

  function toggleTheme() {
    const nextTheme: Theme = theme === "light" ? "dark" : "light";
    document.documentElement.dataset.theme = nextTheme;
    window.localStorage.setItem("mads-docs-theme", nextTheme);
    setTheme(nextTheme);
  }

  return (
    <button className="icon-button" type="button" onClick={toggleTheme} aria-label="Toggle color theme">
      {theme === "light" ? "◐" : "◑"}
    </button>
  );
}
