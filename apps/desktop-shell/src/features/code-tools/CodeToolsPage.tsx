import { useCallback, useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Alert, Button, Checkbox, Input, Select, Space, message } from "antd";
import { Download, FolderOpen, Terminal, X } from "lucide-react";
import styled from "styled-components";
import {
  buildCodeToolsProviderCatalog,
  CLI_TOOLS,
  CLAUDE_CODE,
  filterProvidersForTool,
  GEMINI_CLI,
  GITHUB_COPILOT_CLI,
  getCodeToolModelUniqId,
  OPENAI_CODEX,
  parseEnvironmentVariables,
  type CodeToolId,
} from "@/features/code-tools";
import { AnthropicProviderListPopover } from "@/features/code-tools/components/AnthropicProviderListPopover";
import {
  findSelectedModel,
  ModelSelector,
} from "@/features/code-tools/components/ModelSelector";
import { useCodeTools } from "@/hooks/useCodeTools";
import {
  getCodeToolAvailableTerminals,
  getCodexRuntime,
  getManagedProviders,
  getProviderPresets,
  installBunBinary,
  isBinaryExist,
  runCodeTool,
  type CodeToolsTerminalConfig,
} from "@/lib/tauri";

function getErrorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message) {
    return error.message;
  }

  if (typeof error === "string" && error.trim()) {
    return error;
  }

  if (error && typeof error === "object" && "message" in error) {
    const message =
      typeof error.message === "string" ? error.message : JSON.stringify(error.message);
    if (message && message !== "null" && message !== "undefined") {
      return message;
    }
  }

  return fallback;
}

export function CodeToolsPage() {
  const {
    selectedCliTool,
    selectedModel,
    selectedTerminal,
    environmentVariables,
    directories,
    currentDirectory,
    canLaunch,
    setCliTool,
    setModel,
    setTerminal,
    setEnvVars,
    setCurrentDir,
    removeDir,
    selectFolder,
  } = useCodeTools();
  const [api, contextHolder] = message.useMessage();
  const [isBunInstalled, setIsBunInstalled] = useState(false);
  const [isInstallingBun, setIsInstallingBun] = useState(false);
  const [isLaunching, setIsLaunching] = useState(false);
  const [autoUpdateToLatest, setAutoUpdateToLatest] = useState(false);
  const [availableTerminals, setAvailableTerminals] = useState<
    CodeToolsTerminalConfig[]
  >([]);
  const [isLoadingTerminals, setIsLoadingTerminals] = useState(false);

  const presetsQuery = useQuery({
    queryKey: ["code-tools-provider-presets"],
    queryFn: async () => (await getProviderPresets()).presets,
  });
  const managedProvidersQuery = useQuery({
    queryKey: ["code-tools-managed-providers"],
    queryFn: async () => (await getManagedProviders()).providers,
  });
  const codexRuntimeQuery = useQuery({
    queryKey: ["code-tools-codex-runtime"],
    queryFn: async () => (await getCodexRuntime()).runtime,
  });

  const providerCatalog = useMemo(
    () =>
      buildCodeToolsProviderCatalog(
        managedProvidersQuery.data ?? [],
        presetsQuery.data ?? []
      ),
    [managedProvidersQuery.data, presetsQuery.data]
  );
  const availableProviders = useMemo(
    () => filterProvidersForTool(providerCatalog, selectedCliTool),
    [providerCatalog, selectedCliTool]
  );
  const anthropicProviderNames = useMemo(
    () => filterProvidersForTool(providerCatalog, CLAUDE_CODE).map((provider) => provider.name),
    [providerCatalog]
  );

  const selectedModelValue = selectedModel
    ? getCodeToolModelUniqId(selectedModel)
    : undefined;
  const codexAuthReady =
    codexRuntimeQuery.data?.has_chatgpt_tokens ||
    codexRuntimeQuery.data?.has_api_key ||
    false;

  const checkBunInstallation = useCallback(async () => {
    try {
      const installed = await isBinaryExist("bun");
      setIsBunInstalled(installed);
    } catch {
      setIsBunInstalled(false);
    }
  }, []);

  const loadAvailableTerminals = useCallback(async () => {
    try {
      setIsLoadingTerminals(true);
      const terminals = await getCodeToolAvailableTerminals();
      setAvailableTerminals(terminals);
      if (
        terminals.length > 0 &&
        !terminals.some((terminal) => terminal.id === selectedTerminal)
      ) {
        setTerminal(terminals[0].id);
      }
    } catch {
      setAvailableTerminals([]);
    } finally {
      setIsLoadingTerminals(false);
    }
  }, [selectedTerminal, setTerminal]);

  useEffect(() => {
    void checkBunInstallation();
    void loadAvailableTerminals();
  }, [checkBunInstallation, loadAvailableTerminals]);

  const handleModelChange = (value: string | undefined) => {
    setModel(findSelectedModel(availableProviders, value));
  };

  const handleInstallBun = async () => {
    setIsInstallingBun(true);
    try {
      await installBunBinary();
      api.success("Bun 安装完成");
      await checkBunInstallation();
    } catch (error) {
      api.error(getErrorMessage(error, "安装 Bun 失败"));
    } finally {
      setIsInstallingBun(false);
    }
  };

  const handleSelectFolder = async () => {
    try {
      await selectFolder();
    } catch (error) {
      api.error(getErrorMessage(error, "打开文件夹选择器失败，请重试"));
    }
  };

  const handleLaunch = async () => {
    if (!isBunInstalled) {
      api.warning("请先安装 Bun 环境再启动 CLI 工具");
      return;
    }
    if (!currentDirectory) {
      api.warning("请选择工作目录");
      return;
    }
    if (!selectedModel && selectedCliTool !== GITHUB_COPILOT_CLI) {
      api.warning("请选择模型");
      return;
    }

    setIsLaunching(true);
    try {
      const result = await runCodeTool({
        cliTool: selectedCliTool,
        directory: currentDirectory,
        terminal: selectedTerminal,
        autoUpdateToLatest,
        environmentVariables: parseEnvironmentVariables(environmentVariables),
        selectedModel: selectedModel
          ? {
              providerId: selectedModel.providerId,
              providerName: selectedModel.providerName,
              providerType: selectedModel.providerType,
              runtimeTarget: selectedModel.runtimeTarget,
              baseUrl: selectedModel.baseUrl,
              protocol: selectedModel.protocol,
              modelId: selectedModel.modelId,
              displayName: selectedModel.displayName,
              managedProviderId: selectedModel.managedProviderId,
              presetId: selectedModel.presetId,
              hasStoredCredential: selectedModel.hasStoredCredential,
            }
          : null,
      });

      if (result.success) {
        api.success(result.message || "启动成功");
      } else {
        api.error(result.message || "启动失败，请重试");
      }
    } catch (error) {
      api.error(getErrorMessage(error, "启动失败，请重试"));
    } finally {
      setIsLaunching(false);
    }
  };

  const codexNoticeVisible = selectedCliTool === OPENAI_CODEX && !codexAuthReady;
  const shouldShowModelSelector = selectedCliTool !== GITHUB_COPILOT_CLI;
  const shouldShowTerminalSelector = availableTerminals.length > 0;

  return (
    <Container>
      {contextHolder}
      <ContentContainer>
        <MainContent>
          <Title>代码工具</Title>
          <Description>快速启动多个代码 CLI 工具，提高开发效率</Description>

          {!isBunInstalled && (
            <BunInstallAlert>
              <Alert
                type="warning"
                banner
                style={{ borderRadius: "var(--radius)" }}
                message={
                  <AlertContent>
                    <span>运行 CLI 工具需要安装 Bun 环境</span>
                    <Button
                      type="primary"
                      size="small"
                      icon={<Download size={14} />}
                      onClick={handleInstallBun}
                      loading={isInstallingBun}
                    >
                      {isInstallingBun ? "安装中..." : "安装 Bun"}
                    </Button>
                  </AlertContent>
                }
              />
            </BunInstallAlert>
          )}

          {codexNoticeVisible && (
            <BunInstallAlert>
              <Alert
                type="info"
                showIcon
                style={{ borderRadius: "var(--radius)" }}
                message="OpenAI Codex 当前未检测到可用的 Codex 登录态或 API 凭据"
                description="如果使用 OpenAI Codex，建议先在设置中的 Provider 页面完成 Codex 登录。"
              />
            </BunInstallAlert>
          )}

          <SettingsPanel>
            <SettingsItem>
              <div className="settings-label">CLI 工具</div>
              <Select
                style={{ width: "100%" }}
                placeholder="选择要使用的 CLI 工具"
                value={selectedCliTool}
                options={CLI_TOOLS}
                onChange={(value) => setCliTool(value as CodeToolId)}
              />
            </SettingsItem>

            {shouldShowModelSelector && (
              <SettingsItem>
                <div className="settings-label">
                  模型
                  {selectedCliTool === CLAUDE_CODE && (
                    <AnthropicProviderListPopover
                      providerNames={anthropicProviderNames}
                    />
                  )}
                </div>
                <ModelSelector
                  providers={availableProviders}
                  value={selectedModelValue}
                  placeholder="选择要使用的模型"
                  onChange={handleModelChange}
                />
                {availableProviders.length === 0 && (
                  <HelpText>
                    当前没有可用于
                    {selectedCliTool === CLAUDE_CODE
                      ? " Claude Code"
                      : selectedCliTool === GEMINI_CLI
                        ? " Gemini CLI"
                        : " 该工具"}
                    的服务商配置，页面会先显示预设目录。
                  </HelpText>
                )}
              </SettingsItem>
            )}

            <SettingsItem>
              <div className="settings-label">工作目录</div>
              <Space.Compact style={{ width: "100%", display: "flex" }}>
                <Select
                  style={{ flex: 1, width: 480 }}
                  placeholder="选择工作目录"
                  value={currentDirectory || undefined}
                  onChange={setCurrentDir}
                  allowClear
                  showSearch
                  filterOption={(input, option) => {
                    const label =
                      typeof option?.label === "string"
                        ? option.label
                        : String(option?.value ?? "");
                    return label.toLowerCase().includes(input.toLowerCase());
                  }}
                  options={directories.map((directory) => ({
                    value: directory,
                    label: directory,
                  }))}
                  optionRender={(option) => (
                    <OptionRow>
                      <OptionText>{String(option.value)}</OptionText>
                      <X
                        size={14}
                        style={{ marginLeft: 8, cursor: "pointer", color: "#999" }}
                        onClick={(event) => {
                          event.stopPropagation();
                          removeDir(String(option.value));
                        }}
                      />
                    </OptionRow>
                  )}
                />
                <Button onClick={() => void handleSelectFolder()} style={{ width: 120 }}>
                  选择文件夹
                </Button>
              </Space.Compact>
            </SettingsItem>

            <SettingsItem>
              <div className="settings-label">环境变量</div>
              <Input.TextArea
                rows={2}
                value={environmentVariables}
                placeholder={`KEY1=value1\nKEY2=value2`}
                onChange={(event) => setEnvVars(event.target.value)}
                style={{ fontFamily: "monospace" }}
              />
              <HelpText>输入自定义环境变量（每行一个，格式：KEY=value）</HelpText>
            </SettingsItem>

            {shouldShowTerminalSelector && (
              <SettingsItem>
                <div className="settings-label">终端</div>
                <Space.Compact style={{ width: "100%", display: "flex" }}>
                  <Select
                    style={{ flex: 1 }}
                    placeholder="选择终端应用"
                    value={selectedTerminal}
                    loading={isLoadingTerminals}
                    onChange={setTerminal}
                    options={availableTerminals.map((terminal) => ({
                      value: terminal.id,
                      label: terminal.name,
                    }))}
                  />
                  <Button disabled icon={<FolderOpen size={16} />}>
                    终端路径
                  </Button>
                </Space.Compact>
              </SettingsItem>
            )}

            <SettingsItem>
              <div className="settings-label">更新选项</div>
              <Checkbox
                checked={autoUpdateToLatest}
                onChange={(event) => setAutoUpdateToLatest(event.target.checked)}
              >
                检查更新并安装最新版本
              </Checkbox>
            </SettingsItem>
          </SettingsPanel>

          <Button
            type="primary"
            icon={<Terminal size={16} />}
            size="large"
            block
            onClick={handleLaunch}
            loading={isLaunching}
            disabled={!canLaunch || !isBunInstalled}
          >
            {isLaunching ? "启动中..." : "启动"}
          </Button>
        </MainContent>
      </ContentContainer>
    </Container>
  );
}

const Container = styled.div`
  display: flex;
  flex: 1;
  flex-direction: column;
  background: var(--color-background);
`;

const ContentContainer = styled.div`
  display: flex;
  flex: 1;
  overflow-y: auto;
  padding: 28px 0;
`;

const MainContent = styled.div`
  width: 600px;
  margin: auto;
  min-height: fit-content;
`;

const Title = styled.h1`
  font-size: 24px;
  font-weight: 600;
  margin-bottom: 8px;
  color: var(--color-foreground);
`;

const Description = styled.p`
  font-size: 14px;
  color: var(--color-muted-foreground);
  margin-bottom: 32px;
  line-height: 1.5;
`;

const SettingsPanel = styled.div`
  margin-bottom: 32px;
`;

const SettingsItem = styled.div`
  margin-bottom: 24px;

  .settings-label {
    font-size: 14px;
    margin-bottom: 8px;
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--color-foreground);
    font-weight: 500;
  }
`;

const BunInstallAlert = styled.div`
  margin-bottom: 24px;
`;

const AlertContent = styled.div`
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
`;

const HelpText = styled.div`
  font-size: 12px;
  color: var(--color-muted-foreground);
  margin-top: 4px;
`;

const OptionRow = styled.div`
  display: flex;
  justify-content: space-between;
  align-items: center;
`;

const OptionText = styled.span`
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
`;
