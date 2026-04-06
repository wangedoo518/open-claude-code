import { memo, useMemo } from "react";
import { Select } from "@/components/ui/select";
import type { CodeToolsProviderEntry, SelectedCodeToolModel } from "@/features/code-tools";
import { getCodeToolModelUniqId } from "@/features/code-tools";

interface ModelSelectorProps {
  providers: CodeToolsProviderEntry[];
  value?: string;
  placeholder: string;
  onChange: (value: string | undefined) => void;
}

function ModelSelectorComponent({
  providers,
  value,
  placeholder,
  onChange,
}: ModelSelectorProps) {
  const groups = useMemo(
    () =>
      providers
        .filter((provider) => provider.models.length > 0)
        .map((provider) => ({
          providerName: provider.name,
          options: provider.models.map((model) => ({
            value: getCodeToolModelUniqId(model),
            label: `${model.displayName} | ${provider.name}`,
          })),
        })),
    [providers]
  );

  return (
    <Select
      value={value}
      onChange={(event) => onChange(event.target.value || undefined)}
    >
      <option value="">{placeholder}</option>
      {groups.map((group) => (
        <optgroup key={group.providerName} label={group.providerName}>
          {group.options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </optgroup>
      ))}
    </Select>
  );
}

export const ModelSelector = memo(ModelSelectorComponent);

export function findSelectedModel(
  providers: CodeToolsProviderEntry[],
  value: string | undefined
): SelectedCodeToolModel | null {
  if (!value) {
    return null;
  }

  for (const provider of providers) {
    const match = provider.models.find(
      (model) => getCodeToolModelUniqId(model) === value
    );
    if (match) {
      return match;
    }
  }

  return null;
}
