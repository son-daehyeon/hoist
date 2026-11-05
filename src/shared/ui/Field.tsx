import { useId, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes } from "react";
import { Icon } from "./Icon";
import "./Field.css";
import { noAutofill } from "./noAutofill";

type InputProps = Omit<InputHTMLAttributes<HTMLInputElement>, "size">;

function FieldShell({
  id,
  label,
  helper,
  error,
  children,
}: {
  id: string;
  label?: string;
  helper?: ReactNode;
  error?: boolean;
  children: ReactNode;
}) {
  return (
    <div className="bs-field">
      {label && (
        <label htmlFor={id} className="bs-field-label t-label-s">
          {label}
        </label>
      )}
      {children}
      {helper && <div className={`t-caption ${error ? "c-danger" : "c-tertiary"}`}>{helper}</div>}
    </div>
  );
}

export function Field({
  label,
  helper,
  error = false,
  suffix,
  size = "m",
  action,
  ...rest
}: InputProps & {
  label?: string;
  helper?: ReactNode;
  error?: boolean;
  suffix?: string;
  size?: "m" | "l";
  action?: ReactNode;
}) {
  const id = useId();
  return (
    <FieldShell id={id} label={label} helper={helper} error={error}>
      <div className="bs-field-row">
        <div className={`bs-input bs-input-${size} ${error ? "is-error" : ""}`}>
          <input id={id} {...noAutofill} {...rest} />
          {suffix && <span className="t-caption c-tertiary">{suffix}</span>}
        </div>
        {action}
      </div>
    </FieldShell>
  );
}

export function Select({
  label,
  helper,
  size = "m",
  options,
  ...rest
}: Omit<SelectHTMLAttributes<HTMLSelectElement>, "size"> & {
  label?: string;
  helper?: ReactNode;
  size?: "m" | "l";
  options: { value: string; label: string }[];
}) {
  const id = useId();
  return (
    <FieldShell id={id} label={label} helper={helper}>
      <div className={`bs-input bs-input-${size} bs-select`}>
        <select id={id} {...noAutofill} {...rest}>
          {options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
        <Icon name="chevron-down" size={14} />
      </div>
    </FieldShell>
  );
}

export function SearchField({ width = 240, ...rest }: InputProps & { width?: number }) {
  return (
    <div className="bs-input bs-input-m bs-search" style={{ width }}>
      <Icon name="search" size={14} />
      <input type="search" {...noAutofill} {...rest} />
    </div>
  );
}

export function Checkbox({
  checked,
  onChange,
  children,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  children?: ReactNode;
  disabled?: boolean;
}) {
  return (
    <label className={`bs-checkbox t-body-m ${disabled ? "is-disabled" : ""}`}>
      <input
        {...noAutofill}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="bs-checkbox-box">{checked && <Icon name="check" size={12} />}</span>
      {children}
    </label>
  );
}
