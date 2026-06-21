import { CSSProperties, useEffect, useRef, useState } from "react";
import type { AddressHistory } from "../utils/history";

type Props = {
  value: string;
  onChange: (value: string) => void;
  history: AddressHistory;
  placeholder?: string;
  style?: CSSProperties;
  containerStyle?: CSSProperties;
  disabled?: boolean;
  required?: boolean;
  ariaLabel?: string;
  // 选中历史项后的回调（在 onChange 之后触发），可用于自动提交等
  onPick?: (value: string) => void;
};

// 带历史记录下拉的地址输入框。聚焦时显示历史，输入时按子串过滤，
// 支持键盘上下选择、回车确认、Esc 关闭，每项可单独删除，可一键清空。
export default function AddressInput({
  value,
  onChange,
  history,
  placeholder,
  style,
  containerStyle,
  disabled,
  required,
  ariaLabel,
  onPick,
}: Props) {
  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(-1);
  const wrapRef = useRef<HTMLDivElement>(null);

  // 按当前输入过滤历史；输入恰好等于某历史项时仍展示完整列表，便于切换
  const query = value.trim().toLowerCase();
  const filtered =
    query.length === 0 || history.items.every((v) => v === value)
      ? history.items
      : history.items.filter((v) => v.toLowerCase().includes(query));

  // 点击组件外部时关闭下拉
  useEffect(() => {
    if (!open) return;
    function onDocMouseDown(e: MouseEvent) {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", onDocMouseDown);
    return () => document.removeEventListener("mousedown", onDocMouseDown);
  }, [open]);

  function pick(item: string) {
    onChange(item);
    setOpen(false);
    setHighlight(-1);
    onPick?.(item);
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (!open || filtered.length === 0) {
      if (e.key === "ArrowDown" && filtered.length > 0) {
        setOpen(true);
        setHighlight(0);
        e.preventDefault();
      }
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlight((h) => (h + 1) % filtered.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlight((h) => (h <= 0 ? filtered.length - 1 : h - 1));
    } else if (e.key === "Enter") {
      // 仅当有高亮项时拦截回车做选择，否则放行让表单正常提交
      if (highlight >= 0 && highlight < filtered.length) {
        e.preventDefault();
        pick(filtered[highlight]);
      }
    } else if (e.key === "Escape") {
      setOpen(false);
      setHighlight(-1);
    }
  }

  const showDropdown = open && filtered.length > 0;

  return (
    <div className="address-field" style={containerStyle} ref={wrapRef}>
      <input
        value={value}
        onChange={(e) => {
          onChange(e.currentTarget.value);
          setOpen(true);
          setHighlight(-1);
        }}
        onFocus={() => setOpen(true)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        style={{ width: "100%", ...style }}
        disabled={disabled}
        required={required}
        aria-label={ariaLabel}
        autoComplete="off"
        role="combobox"
        aria-expanded={showDropdown}
      />
      {showDropdown && (
        <div className="address-history" role="listbox">
          {filtered.map((item, i) => (
            <div
              key={item}
              role="option"
              aria-selected={i === highlight}
              className={`address-history-item ${i === highlight ? "active" : ""}`}
              onMouseEnter={() => setHighlight(i)}
              // 用 mousedown 抢在 input 失焦前处理，避免下拉先被关闭
              onMouseDown={(e) => {
                e.preventDefault();
                pick(item);
              }}
            >
              <span className="address-history-value">{item}</span>
              <button
                type="button"
                className="address-history-remove"
                title="删除这条记录"
                onMouseDown={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  history.remove(item);
                }}
              >
                ×
              </button>
            </div>
          ))}
          <button
            type="button"
            className="address-history-clear"
            onMouseDown={(e) => {
              e.preventDefault();
              history.clear();
              setOpen(false);
            }}
          >
            清除历史
          </button>
        </div>
      )}
    </div>
  );
}
