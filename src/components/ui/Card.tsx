import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "@/lib/cn";

export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("rounded-lg border border-border bg-surface", className)} {...props} />;
}

interface CardHeaderProps {
  title: ReactNode;
  icon?: ReactNode;
  action?: ReactNode;
  className?: string;
}

export function CardHeader({ title, icon, action, className }: CardHeaderProps) {
  return (
    <div className={cn("flex items-center justify-between gap-2 px-4 pt-3", className)}>
      <h2 className="flex items-center gap-2 text-[12px] font-semibold uppercase tracking-wide text-fg-muted">
        {icon}
        {title}
      </h2>
      {action}
    </div>
  );
}
