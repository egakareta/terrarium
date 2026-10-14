import { Rnd } from "react-rnd";
import type { ReactNode } from "react";
import { ActionIcon } from "@mantine/core";
import { XIcon } from "@phosphor-icons/react";

export function Window({
  title,
  children,
  width = 520,
  height = 420,
  className = "",
  onClose,
}: {
  title: string;
  children: ReactNode;
  width?: number;
  height?: number;
  className?: string;
  onClose?: () => void;
}) {
  return (
    <Rnd
      default={{
        x: 48,
        y: 48,
        width: Math.min(width, innerWidth - 24),
        height: Math.min(height, innerHeight - 24),
      }}
      minWidth={240}
      minHeight={160}
      bounds="window"
      dragHandleClassName="window-title"
      className={`floating-window ${className}`}
    >
      <header className="window-title">
        <strong>{title}</strong>
        {onClose && (
          <ActionIcon aria-label={`Close ${title}`} variant="subtle" onClick={onClose}>
            <XIcon />
          </ActionIcon>
        )}
      </header>
      <div className="window-content">{children}</div>
    </Rnd>
  );
}
