import { useEffect } from 'react';
import { getFirstProjectDestination } from '@/shared/lib/firstProjectDestination';
import { useOrganizationStore } from '@/shared/stores/useOrganizationStore';
import { useUiPreferencesStore } from '@/shared/stores/useUiPreferencesStore';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';

export function RootRedirectPage() {
  const setSelectedOrgId = useOrganizationStore((s) => s.setSelectedOrgId);
  const appNavigation = useAppNavigation();

  useEffect(() => {
    let isActive = true;
    void (async () => {
      const { selectedOrgId, selectedProjectId } =
        useUiPreferencesStore.getState();

      const destination = await getFirstProjectDestination(
        setSelectedOrgId,
        selectedOrgId,
        selectedProjectId
      );
      if (!isActive) {
        return;
      }

      if (destination?.kind === 'project') {
        appNavigation.goToProject(destination.projectId, { replace: true });
        return;
      }

      appNavigation.goToWorkspacesCreate({ replace: true });
    })();

    return () => {};
  }, [appNavigation, setSelectedOrgId]);

  return (
    <div className="h-screen bg-primary flex items-center justify-center">
      <p className="text-low">Loading...</p>
    </div>
  );
}
