import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import {
  ArrowSquareOutIcon,
  ArrowsClockwiseIcon,
  CheckIcon,
  CopyIcon,
  WarningCircleIcon,
} from '@phosphor-icons/react';
import { Switch } from '@vibe/ui/components/Switch';
import { configApi } from '@/shared/lib/api';
import {
  EXECUTOR_INSTALL_INFO,
  type ExecutorInstallInfo,
} from '@/shared/lib/executorInstallInfo';
import { AgentIcon } from '@/shared/components/AgentIcon';
import { useUserSystem } from '@/shared/hooks/useUserSystem';
import { useSettingsMachineClient } from './SettingsHostContext';
import type { AvailabilityInfo, BaseCodingAgent } from 'shared/types';
import { cn } from '@/shared/lib/utils';

const ALL_EXECUTORS = Object.keys(
  EXECUTOR_INSTALL_INFO
) as BaseCodingAgent[];

function StatusBadge({ info }: { info: AvailabilityInfo | undefined }) {
  if (!info) {
    return (
      <span className="text-xs text-low px-2 py-0.5 rounded-full bg-secondary animate-pulse">
        Проверка…
      </span>
    );
  }

  if (info.type === 'NOT_FOUND') {
    return (
      <span className="flex items-center gap-1 text-xs text-low px-2 py-0.5 rounded-full bg-secondary">
        <WarningCircleIcon className="size-3" weight="fill" />
        Не установлен
      </span>
    );
  }

  const label =
    info.type === 'LOGIN_DETECTED' ? 'Авторизован' : 'Установлен';

  return (
    <span className="flex items-center gap-1 text-xs text-success px-2 py-0.5 rounded-full bg-success/10">
      <CheckIcon className="size-3" weight="bold" />
      {label}
    </span>
  );
}

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback for environments without clipboard API
    }
  };

  return (
    <button
      onClick={handleCopy}
      className={cn(
        'p-1 rounded hover:bg-secondary transition-colors',
        copied ? 'text-success' : 'text-low hover:text-normal'
      )}
      title="Скопировать команду"
    >
      {copied ? (
        <CheckIcon className="size-3.5" weight="bold" />
      ) : (
        <CopyIcon className="size-3.5" weight="regular" />
      )}
    </button>
  );
}

function InstallBlock({ info }: { info: ExecutorInstallInfo }) {
  return (
    <div className="mt-2 ml-8 space-y-1.5">
      {info.installCommand && (
        <div className="flex items-center gap-1.5 bg-secondary/60 rounded px-2.5 py-1.5 font-mono text-xs text-normal max-w-sm">
          <span className="flex-1 truncate">{info.installCommand}</span>
          <CopyButton text={info.installCommand} />
        </div>
      )}
      <div className="flex items-center gap-3">
        {info.installNote && (
          <span className="text-xs text-low">{info.installNote}</span>
        )}
        <a
          href={info.installUrl}
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center gap-0.5 text-xs text-brand hover:underline shrink-0"
        >
          Инструкция
          <ArrowSquareOutIcon className="size-3" />
        </a>
      </div>
    </div>
  );
}

interface ExecutorRowProps {
  executor: BaseCodingAgent;
  availability: AvailabilityInfo | undefined;
  isEnabled: boolean;
  onToggle: (executor: BaseCodingAgent, enabled: boolean) => void;
}

function ExecutorRow({
  executor,
  availability,
  isEnabled,
  onToggle,
}: ExecutorRowProps) {
  const installInfo = EXECUTOR_INSTALL_INFO[executor];
  const isAvailable = availability
    ? availability.type !== 'NOT_FOUND'
    : false;

  return (
    <div
      className={cn(
        'rounded-md border p-3 transition-colors',
        isAvailable
          ? 'border-border bg-background'
          : 'border-border/50 bg-secondary/30'
      )}
    >
      <div className="flex items-center gap-3">
        {/* Icon */}
        <AgentIcon
          agent={executor}
          className={cn(
            'size-5 shrink-0',
            !isAvailable && 'opacity-40 grayscale'
          )}
        />

        {/* Name + status */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span
              className={cn(
                'text-sm font-medium',
                !isAvailable ? 'text-low' : 'text-normal'
              )}
            >
              {installInfo.displayName}
            </span>
            <StatusBadge info={availability} />
          </div>
          <p className="text-xs text-low mt-0.5">{installInfo.description}</p>
        </div>

        {/* Toggle — only enabled when installed */}
        <Switch
          checked={isAvailable && isEnabled}
          onCheckedChange={(checked) => onToggle(executor, checked)}
          disabled={!isAvailable}
          title={
            !isAvailable
              ? 'Сначала установите агента'
              : isEnabled
                ? 'Отключить агента'
                : 'Включить агента'
          }
        />
      </div>

      {/* Install instructions when not found */}
      {!isAvailable && <InstallBlock info={installInfo} />}
    </div>
  );
}

export function ExecutorAvailabilityPanel() {
  const machineClient = useSettingsMachineClient();
  const { config, updateAndSaveConfig } = useUserSystem();

  const hostId = machineClient?.target.apiHostId ?? null;

  const {
    data: availability,
    isLoading,
    refetch,
    isFetching,
  } = useQuery({
    queryKey: ['agents-availability', hostId],
    queryFn: () => configApi.getAllAgentsAvailability(hostId),
    staleTime: 30_000,
  });

  const disabledSet = new Set<string>(config?.disabled_executors ?? []);

  const handleToggle = async (
    executor: BaseCodingAgent,
    enabled: boolean
  ) => {
    if (!config) return;
    const next = new Set(disabledSet);
    if (enabled) {
      next.delete(executor);
    } else {
      next.add(executor);
    }
    await updateAndSaveConfig({
      disabled_executors: Array.from(next) as BaseCodingAgent[],
    });
  };

  return (
    <div className="space-y-3">
      {/* Header row */}
      <div className="flex items-center justify-between">
        <p className="text-xs text-low">
          Агенты без галочки не будут доступны при создании задачи.
        </p>
        <button
          onClick={() => refetch()}
          disabled={isFetching}
          className="flex items-center gap-1 text-xs text-low hover:text-normal transition-colors disabled:opacity-50"
          title="Проверить снова"
        >
          <ArrowsClockwiseIcon
            className={cn('size-3.5', isFetching && 'animate-spin')}
            weight="bold"
          />
          Проверить снова
        </button>
      </div>

      {/* Executor cards */}
      <div className="space-y-2">
        {ALL_EXECUTORS.map((executor) => (
          <ExecutorRow
            key={executor}
            executor={executor}
            availability={
              isLoading ? undefined : availability?.[executor]
            }
            isEnabled={!disabledSet.has(executor)}
            onToggle={handleToggle}
          />
        ))}
      </div>
    </div>
  );
}
