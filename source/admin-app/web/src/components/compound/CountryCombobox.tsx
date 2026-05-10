import { Combobox, ComboboxItem } from "@wardnet/forge-web/combobox";
import { countryFlag } from "@/lib/country";
import type { CountryInfo } from "@wardnet/js";

interface CountryComboboxProps {
  countries: CountryInfo[];
  value: string;
  onChange: (code: string) => void;
  placeholder?: string;
  disabled?: boolean;
}

/** Searchable country picker (Combobox composite). */
export function CountryCombobox({
  countries,
  value,
  onChange,
  placeholder = "All countries",
  disabled,
}: CountryComboboxProps) {
  const selected = countries.find((c) => c.code === value);

  const trigger = selected ? (
    <span className="flex items-center gap-2">
      <span>{countryFlag(selected.code)}</span>
      {selected.name}
    </span>
  ) : (
    <span className="text-ink-3">{placeholder}</span>
  );

  return (
    <Combobox
      value={value}
      onChange={(next) => onChange(next === value ? "" : next)}
      trigger={trigger}
      searchPlaceholder="Search country…"
      empty="No country found."
      disabled={disabled}
    >
      {countries.map((c) => (
        <ComboboxItem key={c.code} value={c.code} keywords={[c.name]}>
          <span>{countryFlag(c.code)}</span>
          {c.name}
        </ComboboxItem>
      ))}
    </Combobox>
  );
}
