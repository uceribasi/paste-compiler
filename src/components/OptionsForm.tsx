import { useI18n } from "../i18n";
import type { OutputOptions } from "../types";
import { optionFieldsFor } from "../utils";

interface Props {
  formatId: string;
  options: OutputOptions;
  onChange: (options: OutputOptions) => void;
}

export default function OptionsForm({ formatId, options, onChange }: Props) {
  const { lang, t } = useI18n();
  const fields = optionFieldsFor(formatId, lang);
  if (fields.length === 0) return null;
  const set = (key: keyof OutputOptions, value: unknown) => onChange({ ...options, [key]: value });
  const choiceLabel = (c: string) => (c === "\t" ? t("opt.choice.tab") : c === ": " ? t("opt.choice.colonSpace") : c === " = " ? t("opt.choice.spacedEquals") : c);
  return (
    <div className="options-row">
      {fields.map((f) => {
        const value = options[f.key];
        switch (f.type) {
          case "boolean":
            return (
              <label key={f.key} className="checkbox" title={f.help}>
                <input type="checkbox" checked={Boolean(value ?? f.defaultValue ?? false)} onChange={(e) => set(f.key, e.target.checked)} />
                {f.label}
              </label>
            );
          case "number":
            return (
              <label key={f.key} className="field inline" title={f.help}>
                <span>{f.label}</span>
                <input
                  type="number"
                  min={0}
                  max={8}
                  value={value === undefined || value === null ? "" : String(value)}
                  placeholder={f.placeholder}
                  onChange={(e) => set(f.key, e.target.value === "" ? undefined : Number(e.target.value))}
                />
              </label>
            );
          case "select":
            return (
              <label key={f.key} className="field inline" title={f.help}>
                <span>{f.label}</span>
                <select value={String(value ?? f.choices?.[0] ?? "")} onChange={(e) => set(f.key, e.target.value)}>
                  {f.choices?.map((c) => (
                    <option key={c} value={c}>
                      {choiceLabel(c)}
                    </option>
                  ))}
                </select>
              </label>
            );
          default:
            return (
              <label key={f.key} className="field inline" title={f.help}>
                <span>{f.label}</span>
                <input type="text" value={String(value ?? "")} placeholder={f.placeholder} onChange={(e) => set(f.key, e.target.value)} spellCheck={false} />
              </label>
            );
        }
      })}
    </div>
  );
}
