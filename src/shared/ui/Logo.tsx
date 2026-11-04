import { Icon } from "./Icon";
import "./Logo.css";

export function Logo({ size = 16, tile = false }: { size?: number; tile?: boolean }) {
  if (!tile) return <Icon name="hoist" size={size} />;
  return (
    <span className="bs-logo-tile" style={{ width: size, height: size }}>
      <Icon name="hoist" size={Math.round(size * 0.62)} />
    </span>
  );
}
