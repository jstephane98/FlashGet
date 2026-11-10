import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "../../api";
import type { Category, CategoryDirs } from "../../types";
import { t } from "../../i18n";
import type { MsgKey } from "../../i18n";
import { Card, SectionShell, SettingRow, ghostBtn } from "./controls";

const CATS: Category[] = ["video", "document", "program", "archive", "music", "other"];

export function FoldersSection() {
  const [dirs, setDirs] = useState<CategoryDirs | null>(null);
  const [error, setError] = useState<string>();

  useEffect(() => {
    let alive = true;
    api.getCategoryDirs().then((d) => { if (alive) setDirs(d); }).catch((e) => { if (alive) setError(String(e)); });
    return () => { alive = false; };
  }, []);

  const apply = async (cat: Category, path: string | null) => {
    setError(undefined);
    try {
      await api.setCategoryDir(cat, path);
      setDirs((d) => (d ? { ...d, [cat]: path } : d));
    } catch (e) {
      setError(String(e));
    }
  };

  const browse = async (cat: Category) => {
    try {
      const picked = await open({ directory: true });
      if (typeof picked === "string" && picked) await apply(cat, picked);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <SectionShell title={t("set.nav.folders")} desc={t("set.folders.desc")} error={error}>
      <Card title={t("set.foldersCard")}>
        {CATS.map((cat) => {
          const label = t(`set.cat.${cat}` as MsgKey);
          const path = dirs?.[cat] ?? null;
          return (
            <SettingRow key={cat} label={label}>
              <span title={path ?? undefined}
                className="px-2.5 py-1.5 rounded-[7px] bg-field border border-border text-[12px] font-mono text-ink-soft max-w-[280px] truncate min-w-[120px]">
                {path ?? "—"}
              </span>
              <button type="button" className={ghostBtn} onClick={() => browse(cat)} aria-label={t("set.browse")}>{t("set.browse")}</button>
              <button type="button" className={ghostBtn} disabled={!path} onClick={() => apply(cat, null)} aria-label={t("set.default")}>{t("set.default")}</button>
            </SettingRow>
          );
        })}
      </Card>
      <p className="text-[12px] text-ink-mute">{t("set.foldersHint")}</p>
    </SectionShell>
  );
}
