import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../../i18n";
import { BrandMark } from "./BrandMark";

const btn =
  "flex h-full w-11 items-center justify-center text-ink-soft transition-colors hover:bg-surface hover:text-ink";

export function TitleBar() {
  const win = getCurrentWindow();
  return (
    <div
      data-tauri-drag-region
      className="flex h-9 shrink-0 select-none items-center justify-between border-b border-border bg-deep font-sans"
    >
      <div data-tauri-drag-region className="flex items-center gap-2 pl-3">
        <BrandMark size={20} className="pointer-events-none" />
        <span data-tauri-drag-region className="text-[13px] font-semibold text-ink">
          FlashGet
        </span>
      </div>
      <div className="flex h-full">
        <button
          type="button"
          className={btn}
          aria-label={t("window.minimize")}
          title={t("window.minimize")}
          onClick={() => void win.minimize()}
        >
          <Minus size={14} />
        </button>
        <button
          type="button"
          className={btn}
          aria-label={t("window.maximize")}
          title={t("window.maximize")}
          onClick={() => void win.toggleMaximize()}
        >
          <Square size={11} />
        </button>
        <button
          type="button"
          className={`${btn} hover:bg-err hover:text-white`}
          aria-label={t("window.close")}
          title={t("window.close")}
          onClick={() => void win.close()}
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
