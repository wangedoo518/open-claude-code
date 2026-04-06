import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { Download, FolderOpen, Loader2, Terminal, X } from "lucide-react";
import { toast } from "sonner";
import {
  buildCodeToolsProviderCatalog,
  CLI_TOOLS,
  findPreferredCodeToolModel,
  filterProvidersForTool,
  getCodeToolLabel,
  getCodeToolModelUniqId,
  OPENAI_CODEX,
  parseEnvironmentVariables,
  toolRequiresModel,
  toolUsesManagedAuth,
  type CodeToolId,
} from "@/features/code-tools";
import {
  findSelectedModel,
  ModelSelector,
} from "@/features/code-tools/components/ModelSelector";
import { useCodeTools } from "@/hooks/useCodeTools";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import {
  getCodeToolAvailableTerminals,
  getCodexRuntime,
  getManagedAuthProviders,
  installBunBinary,
  isBinaryExist,
  runCodeTool,
  type CodeToolsTerminalConfig,
} from "@/lib/tauri";
import { codeToolsKeys } from "./api/query";

function getErrorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message) {
    return error.message;
  }

  if (typeof error === "string" && error.trim()) {
    return error;
  }

  if (error && typeof error === "object" && "message" in error) {
    const msg =
      typeof error.message === "string" ? error.message : JSON.stringify(error.message);
    if (msg && msg !== "null" && msg !== "undefined") {
      return msg;
    }
  }

  return fallback;
}

export function CodeToolsPage() {
  const { t } = useTranslation();
  const {
    selectedCliTool,
    selectedModel,
    selectedTerminal,
    environmentVariables,
    directories,
    currentDirectory,
    setCliTool,
    setModel,
    setTerminal,
    setEnvVars,
    setCurrentDir,
    removeDir,
    selectFolder,
  } = useCodeTools();
  const [isBunInstalled, setIsBunInstalled] = useState(false);
  const [isInstallingBun, setIsInstallingBun] = useState(false);
  const [isLaunching, setIsLaunching] = useState(false);
  const [autoUpdateToLatest, setAutoUpdateToLatest] = useState(false);
  const [availableTerminals, setAvailableTerminals] = useState<
    CodeToolsTerminalConfig[]
  >([]);
  const [isLoadingTerminals, setIsLoadingTerminals] = useState(false);

  const managedAuthProvidersQuery = useQuery({
    queryKey: codeToolsKeys.managedAuthProviders(),
    queryFn: async () => (await getManagedAuthProviders()).providers,
  });
  const codexRuntimeQuery = useQuery({
    queryKey: codeToolsKeys.codexRuntime(),
    queryFn: async () => (await getCodexRuntime()).runtime,
  });

  const providerCatalog = useMemo(
    () => buildCodeToolsProviderCatalog(managedAuthProvidersQuery.data ?? []),
    [managedAuthProvidersQuery.data]
  );
  const availableProviders = useMemo(
    () => filterProvidersForTool(providerCatalog, selectedCliTool),
    [providerCatalog, selectedCliTool]
  );
  const requiresModel = toolRequiresModel(selectedCliTool);
  const usesManagedAuth = toolUsesManagedAuth(selectedCliTool);

  const selectedModelValue = selectedModel
    ? getCodeToolModelUniqId(selectedModel)
    : undefined;
  const selectedAvailableModel = useMemo(
    () => findSelectedModel(availableProviders, selectedModelValue),
    [availableProviders, selectedModelValue]
  );
  const effectiveSelectedModel = selectedAvailableModel ?? selectedModel;
  const canLaunchWithCurrentSelection = Boolean(
    selectedCliTool &&
      currentDirectory &&
      (!requiresModel ||
        (effectiveSelectedModel && effectiveSelectedModel.hasStoredCredential))
  );
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

  useEffect(() => {
    if (!requiresModel) {
      return;
    }

    if (availableProviders.length === 0) {
      if (selectedModel) {
        setModel(null);
      }
      return;
    }

    if (selectedAvailableModel) {
      return;
    }

    const preferredModel = findPreferredCodeToolModel(
      availableProviders,
      selectedCliTool
    );
    setModel(preferredModel);
  }, [
    availableProviders,
    selectedAvailableModel,
    selectedCliTool,
    selectedModel,
    setModel,
    requiresModel,
  ]);

  useEffect(() => {
    if (!selectedAvailableModel || !selectedModel) {
      return;
    }

    if (
      selectedAvailableModel.hasStoredCredential ===
        selectedModel.hasStoredCredential &&
      selectedAvailableModel.displayName === selectedModel.displayName &&
      selectedAvailableModel.providerId === selectedModel.providerId &&
      selectedAvailableModel.modelId === selectedModel.modelId
    ) {
      return;
    }

    setModel(selectedAvailableModel);
  }, [selectedAvailableModel, selectedModel, setModel]);

  const handleModelChange = (value: string | undefined) => {
    setModel(findSelectedModel(availableProviders, value));
  };

  const handleInstallBun = async () => {
    setIsInstallingBun(true);
    try {
      await installBunBinary();
      toast.success(t("codetools.success.bunInstalled"));
      await checkBunInstallation();
    } catch (error) {
      toast.error(getErrorMessage(error, t("codetools.error.installBunFailed")));
    } finally {
      setIsInstallingBun(false);
    }
  };

  const handleSelectFolder = async () => {
    try {
      await selectFolder();
    } catch (error) {
      toast.error(getErrorMessage(error, t("codetools.error.folderSelectorFailed")));
    }
  };

  const handleLaunch = async () => {
    if (!isBunInstalled) {
      toast.warning(t("codetools.warning.bunRequired"));
      return;
    }
    if (!currentDirectory) {
      toast.warning(t("codetools.warning.workdirRequired"));
      return;
    }
    if (requiresModel && !effectiveSelectedModel) {
      toast.warning(t("codetools.warning.modelRequired"));
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
        selectedModel: effectiveSelectedModel
          ? {
              providerId: effectiveSelectedModel.providerId,
              providerName: effectiveSelectedModel.providerName,
              providerType: effectiveSelectedModel.providerType,
              baseUrl: effectiveSelectedModel.baseUrl,
              protocol: effectiveSelectedModel.protocol,
              modelId: effectiveSelectedModel.modelId,
              displayName: effectiveSelectedModel.displayName,
              hasStoredCredential: effectiveSelectedModel.hasStoredCredential,
            }
          : null,
      });

      if (result.success) {
        toast.success(result.message || t("codetools.success.launchSuccess"));
      } else {
        toast.error(result.message || t("codetools.error.launchFailed"));
      }
    } catch (error) {
      toast.error(getErrorMessage(error, t("codetools.error.launchFailed")));
    } finally {
      setIsLaunching(false);
    }
  };

  const codexNoticeVisible = selectedCliTool === OPENAI_CODEX && !codexAuthReady;
  const shouldShowTerminalSelector = availableTerminals.length > 0;
  const selectedToolLabel = getCodeToolLabel(selectedCliTool);
  const noProvidersToolLabel = selectedToolLabel;

  return (
    <div className="flex flex-1 flex-col bg-background">
      <div className="flex flex-1 overflow-y-auto py-7">
        <div className="mx-auto min-h-fit w-[600px]">
          <h1 className="mb-2 text-xl font-semibold text-foreground">
            {t("codetools.page.title")}
          </h1>
          <p className="mb-8 text-body leading-relaxed text-muted-foreground">
            {t("codetools.page.description")}
          </p>

          {!isBunInstalled && (
            <div className="mb-6 flex items-center justify-between gap-3 rounded-lg border border-amber-500/30 bg-amber-500/5 px-4 py-3 text-body">
              <span className="text-amber-700 dark:text-amber-400">
                {t("codetools.notice.bunRequired")}
              </span>
              <Button
                size="sm"
                onClick={() => void handleInstallBun()}
                disabled={isInstallingBun}
              >
                {isInstallingBun ? (
                  <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                ) : (
                  <Download className="mr-1.5 size-3.5" />
                )}
                {isInstallingBun ? t("codetools.button.installing") : t("codetools.button.installBun")}
              </Button>
            </div>
          )}

          {codexNoticeVisible && (
            <div className="mb-6 rounded-lg border border-blue-500/30 bg-blue-500/5 px-4 py-3">
              <div className="text-body font-medium text-blue-700 dark:text-blue-400">
                {t("codetools.notice.codexAuthMissing")}
              </div>
              <div className="mt-1 text-body-sm text-blue-600/80 dark:text-blue-400/70">
                {t("codetools.notice.codexAuthHint")}
              </div>
            </div>
          )}

          <div className="mb-8 space-y-6">
            {/* CLI Tool */}
            <div>
              <div className="mb-2 flex items-center gap-2 text-body font-medium text-foreground">
                {t("codetools.label.cliTool")}
              </div>
              <Select
                value={selectedCliTool}
                onChange={(event) => setCliTool(event.target.value as CodeToolId)}
              >
                <option value="" disabled>
                  {t("codetools.placeholder.selectTool")}
                </option>
                {CLI_TOOLS.map((tool) => (
                  <option key={tool.value} value={tool.value}>
                    {tool.label}
                  </option>
                ))}
              </Select>
            </div>

            {/* Model */}
            <div>
              <div className="mb-2 flex items-center gap-2 text-body font-medium text-foreground">
                {t("codetools.label.model")}
              </div>
              {requiresModel ? (
                <>
                  <ModelSelector
                    providers={availableProviders}
                    value={selectedModelValue}
                    placeholder={t("codetools.placeholder.selectModel")}
                    onChange={handleModelChange}
                  />
                  {availableProviders.length === 0 && (
                    <p className="mt-1 text-body-sm text-muted-foreground">
                      {t("codetools.notice.noProviders")} {noProvidersToolLabel}
                      {t("codetools.notice.presetsShown")}
                    </p>
                  )}
                </>
              ) : (
                <div className="rounded-md border border-dashed border-border bg-muted/30 px-3 py-3 text-body-sm text-muted-foreground">
                  <div>{t("codetools.notice.modelOptional")}</div>
                  <div className="mt-1">
                    {usesManagedAuth
                      ? t("codetools.notice.managedAuthLaunch", {
                          tool: selectedToolLabel,
                        })
                      : t("codetools.notice.localCliLaunch", {
                          tool: selectedToolLabel,
                        })}
                  </div>
                </div>
              )}
            </div>

            {/* Working directory */}
            <div>
              <div className="mb-2 flex items-center gap-2 text-body font-medium text-foreground">
                {t("codetools.label.workdir")}
              </div>
              <div className="flex w-full items-center gap-2">
                <Select
                  className="min-w-0 flex-1"
                  value={currentDirectory || ""}
                  onChange={(event) => setCurrentDir(event.target.value)}
                >
                  <option value="">
                    {t("codetools.placeholder.selectWorkdir")}
                  </option>
                  {directories.map((directory) => (
                    <option key={directory} value={directory}>
                      {directory}
                    </option>
                  ))}
                </Select>
                <Button
                  variant="outline"
                  className="shrink-0"
                  onClick={() => void handleSelectFolder()}
                >
                  {t("codetools.button.selectFolder")}
                </Button>
              </div>
              {directories.length > 0 && (
                <div className="mt-2 flex flex-wrap gap-2">
                  {directories.map((directory) => (
                    <button
                      key={directory}
                      type="button"
                      className="inline-flex items-center gap-1 rounded-full border border-border bg-muted/20 px-2.5 py-1 text-xs text-muted-foreground transition hover:bg-muted/30 hover:text-foreground"
                      onClick={() => setCurrentDir(directory)}
                    >
                      <span className="max-w-[280px] truncate">{directory}</span>
                      <X
                        size={12}
                        onClick={(event) => {
                          event.stopPropagation();
                          removeDir(directory);
                        }}
                      />
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* Environment variables */}
            <div>
              <div className="mb-2 flex items-center gap-2 text-body font-medium text-foreground">
                {t("codetools.label.envVars")}
              </div>
              <textarea
                rows={2}
                value={environmentVariables}
                placeholder={`KEY1=value1\nKEY2=value2`}
                onChange={(event) => setEnvVars(event.target.value)}
                className="w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-body text-foreground placeholder:text-muted-foreground focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring"
              />
              <p className="mt-1 text-body-sm text-muted-foreground">
                {t("codetools.hint.envVars")}
              </p>
            </div>

            {/* Terminal */}
            {shouldShowTerminalSelector && (
              <div>
                <div className="mb-2 flex items-center gap-2 text-body font-medium text-foreground">
                  {t("codetools.label.terminal")}
                </div>
                <div className="flex w-full items-center gap-2">
                  <Select
                    className="min-w-0 flex-1"
                    value={selectedTerminal}
                    onChange={(event) => setTerminal(event.target.value)}
                    disabled={isLoadingTerminals}
                  >
                    <option value="">
                      {t("codetools.placeholder.selectTerminal")}
                    </option>
                    {availableTerminals.map((terminal) => (
                      <option key={terminal.id} value={terminal.id}>
                        {terminal.name}
                      </option>
                    ))}
                  </Select>
                  <Button variant="outline" disabled className="shrink-0">
                    <FolderOpen className="mr-1.5 size-4" />
                    {t("codetools.button.terminalPath")}
                  </Button>
                </div>
              </div>
            )}

            {/* Update option */}
            <div>
              <div className="mb-2 flex items-center gap-2 text-body font-medium text-foreground">
                {t("codetools.label.updateOptions")}
              </div>
              <label className="flex cursor-pointer items-center gap-2.5 text-body text-foreground">
                <input
                  type="checkbox"
                  checked={autoUpdateToLatest}
                  onChange={(event) =>
                    setAutoUpdateToLatest(event.target.checked)
                  }
                  className="size-4 rounded border-border accent-primary"
                />
                {t("codetools.checkbox.autoUpdate")}
              </label>
            </div>
          </div>

            <Button
            className="h-10 w-full"
            onClick={() => void handleLaunch()}
            disabled={!canLaunchWithCurrentSelection || !isBunInstalled || isLaunching}
          >
            {isLaunching ? (
              <Loader2 className="mr-2 size-4 animate-spin" />
            ) : (
              <Terminal className="mr-2 size-4" />
            )}
            {isLaunching ? t("codetools.button.launching") : t("codetools.button.launch")}
          </Button>
        </div>
      </div>
    </div>
  );
}
