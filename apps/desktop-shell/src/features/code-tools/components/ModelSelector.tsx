import { memo, useMemo } from "react";
import { Select } from "antd";
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
  const options = useMemo(
    () =>
      providers.flatMap((provider) => {
        if (provider.models.length === 0) {
          return [];
        }
        return [
          {
            label: provider.name,
            title: provider.name,
            options: provider.models.map((model) => ({
              value: getCodeToolModelUniqId(model),
              title: `${model.displayName} | ${provider.name}`,
              label: (
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span>{model.displayName}</span>
                  <span style={{ opacity: 0.45 }}>{`| ${provider.name}`}</span>
                </div>
              ),
            })),
          },
        ];
      }),
    [providers]
  );

  return (
    <Select
      showSearch
      allowClear
      style={{ width: "100%" }}
      value={value}
      options={options}
      placeholder={placeholder}
      filterOption={(input, option) => {
        const target =
          typeof option?.title === "string"
            ? option.title
            : "";
        return target.toLowerCase().includes(input.toLowerCase());
      }}
      onChange={(nextValue) => onChange(nextValue)}
    />
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
