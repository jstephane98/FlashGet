/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "#0f172a",
        deep: "#020617",
        surface: "#1e293b",
        border: "#334155",
        ink: "#e2e8f0",
        "ink-soft": "#94a3b8",
        "ink-mute": "#64748b",
        accent: {
          DEFAULT: "#f59e0b",
          soft: "#fbbf24",
          bright: "#fcd34d",
          ink: "#3b1d00",
        },
        ok: "#34d399",
        okdim: "#10b981",
        err: "#f87171",
        queued: "#475569",
      },
      fontFamily: {
        sans: ["Archivo", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ['"JetBrains Mono"', "ui-monospace", "monospace"],
      },
    },
  },
  plugins: [],
};
