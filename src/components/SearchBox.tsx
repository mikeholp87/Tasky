import { Search, X } from "lucide-react";

interface SearchBoxProps {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  label: string;
  className?: string;
}

/** Search input with a clear button; Escape clears it. */
export function SearchBox({ value, onChange, placeholder, label, className }: SearchBoxProps) {
  return (
    <div className={`relative ${className ?? "w-72"}`}>
      <Search
        size={14}
        aria-hidden
        className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-fg-subtle"
      />
      <input
        type="search"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape" && value) {
            e.stopPropagation();
            onChange("");
          }
        }}
        placeholder={placeholder}
        aria-label={label}
        className="h-8 w-full rounded-md border border-border bg-surface pl-8 pr-7 outline-none placeholder:text-fg-subtle focus:border-accent [&::-webkit-search-cancel-button]:hidden"
      />
      {value && (
        <button
          type="button"
          aria-label="Clear search"
          onClick={() => onChange("")}
          className="absolute right-1.5 top-1/2 grid size-5 -translate-y-1/2 place-items-center rounded text-fg-subtle hover:text-fg"
        >
          <X size={13} />
        </button>
      )}
    </div>
  );
}
