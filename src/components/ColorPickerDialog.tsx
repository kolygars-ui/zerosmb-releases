import { useEffect, useState } from "react";
import { hexToRgb, rgbToHex } from "../theme";

interface Props {
  open: boolean;
  initialColor: string;
  onApply: (color: string) => void;
  onCancel: () => void;
}

const channels = [
  ["r", "R"],
  ["g", "G"],
  ["b", "B"],
] as const;

export default function ColorPickerDialog({
  open,
  initialColor,
  onApply,
  onCancel,
}: Props) {
  const [color, setColor] = useState({ r: 8, g: 12, b: 24 });

  useEffect(() => {
    if (open) setColor(hexToRgb(initialColor));
  }, [open, initialColor]);

  if (!open) return null;

  const hex = rgbToHex(color.r, color.g, color.b);

  const update = (key: "r" | "g" | "b", value: number) => {
    setColor((current) => ({
      ...current,
      [key]: Math.max(0, Math.min(255, Number(value) || 0)),
    }));
  };

  return (
    <div
      className="confirm-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onCancel();
      }}
    >
      <div className="confirm-card color-picker-card" role="dialog" aria-modal="true">
        <h3>Цвет оформления</h3>
        <p>Настройте фон приложения ползунками RGB.</p>
        <div className="rgb-layout">
          <div className="rgb-sliders">
            {channels.map(([key, label]) => (
              <div className="rgb-row" key={key}>
                <span className="rgb-label">{label}</span>
                <input
                  type="range"
                  min={0}
                  max={255}
                  value={color[key]}
                  onChange={(event) => update(key, Number(event.target.value))}
                />
                <input
                  className="rgb-value"
                  type="number"
                  min={0}
                  max={255}
                  value={color[key]}
                  onChange={(event) => update(key, Number(event.target.value))}
                />
              </div>
            ))}
          </div>
          <div className="rgb-preview-wrap">
            <div className="rgb-preview" style={{ background: hex }} />
            <span className="mono">{hex}</span>
          </div>
        </div>
        <div className="row end" style={{ marginTop: 18 }}>
          <button className="btn" onClick={onCancel}>
            Отмена
          </button>
          <button className="btn primary" onClick={() => onApply(hex)}>
            Применить
          </button>
        </div>
      </div>
    </div>
  );
}
