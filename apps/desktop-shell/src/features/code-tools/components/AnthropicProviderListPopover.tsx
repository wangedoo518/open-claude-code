import { Popover } from "antd";
import { HelpCircle } from "lucide-react";
import styled from "styled-components";

interface AnthropicProviderListPopoverProps {
  providerNames: string[];
}

export function AnthropicProviderListPopover({
  providerNames,
}: AnthropicProviderListPopoverProps) {
  const content = (
    <PopoverContent>
      <PopoverTitle>支持的服务商</PopoverTitle>
      <ProviderList>
        {providerNames.length > 0 ? (
          providerNames.map((providerName) => (
            <ProviderItem key={providerName}>{providerName}</ProviderItem>
          ))
        ) : (
          <ProviderItem>暂无 Anthropic 兼容服务商</ProviderItem>
        )}
      </ProviderList>
    </PopoverContent>
  );

  return (
    <Popover content={content} trigger="hover" placement="right">
      <HelpCircle
        size={14}
        style={{ color: "var(--color-muted-foreground)", cursor: "pointer" }}
      />
    </Popover>
  );
}

const PopoverContent = styled.div`
  width: 220px;
`;

const PopoverTitle = styled.div`
  margin-bottom: 8px;
  font-weight: 500;
`;

const ProviderList = styled.div`
  display: flex;
  flex-direction: column;
  gap: 6px;
`;

const ProviderItem = styled.div`
  color: var(--color-foreground);
  font-size: 13px;
`;
