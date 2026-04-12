import { useState, useMemo } from 'react';
import { Button } from '@vibe/ui/components/Button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@vibe/ui/components/KeyboardDialog';
import { create, useModal } from '@ebay/nice-modal-react';

import { defineModal } from '@/shared/lib/modals';
import { useShape } from '@/shared/integrations/electric/hooks';
import { PROJECTS_SHAPE, PROJECT_MUTATION, type Project } from 'shared/remote-types';
import { getRandomPresetColor } from '@/shared/lib/colors';
import { FolderPlusIcon, FolderOpenIcon } from '@phosphor-icons/react';
import { FolderPickerDialog } from '@/shared/dialogs/shared/FolderPickerDialog';
import { setActiveProjectPath, getActiveProjectPath } from '@/shared/lib/remoteApi';

export type CreateRemoteProjectDialogProps = {
  organizationId: string;
};

export type CreateRemoteProjectResult = {
  action: 'created' | 'canceled';
  project?: Project;
};

const CreateRemoteProjectDialogImpl = create<CreateRemoteProjectDialogProps>(
  ({ organizationId }) => {
    const modal = useModal();

    const [error, setError] = useState<string | null>(null);
    const [isCreating, setIsCreating] = useState(false);

    const params = useMemo(
      () => ({ organization_id: organizationId }),
      [organizationId]
    );

    const { insert } = useShape(PROJECTS_SHAPE, params, {
      mutation: PROJECT_MUTATION,
    });

    const handleSelectFolder = async () => {
      // Snapshot the previous active path so we can restore it on failure.
      const previousPath = getActiveProjectPath();

      try {
        const path = await FolderPickerDialog.show({});
        if (path) {
          setIsCreating(true);
          // Persist as active project path for task creation defaults.
          // Must be set BEFORE insert() so the offline mock can resolve project_id.
          setActiveProjectPath(path);

          const { data: project, persisted } = insert({
            id: path, // Absolute directory path IS the project_id in local-first mode
            organization_id: organizationId,
            name: path.split('/').pop() || path, // basename as display name
            color: getRandomPresetColor(),
            sort_order: 0,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          } as any);

          const persistedProject = await persisted;
          modal.resolve({ action: 'created', project: persistedProject ?? project } as CreateRemoteProjectResult);
          modal.hide();
        }
      } catch (err) {
        // Restore the previously active path so no tasks are orphaned into an
        // uncommitted project bucket.
        if (previousPath !== null) {
          setActiveProjectPath(previousPath);
        }
        setError(err instanceof Error ? err.message : 'Failed to select folder');
        setIsCreating(false);
      }
    };

    const handleCancel = () => {
      modal.resolve({ action: 'canceled' } as CreateRemoteProjectResult);
      modal.hide();
    };

    const handleOpenChange = (open: boolean) => {
      if (isCreating) return;
      if (!open) {
        handleCancel();
      }
    };

    return (
      <Dialog open={modal.visible} onOpenChange={handleOpenChange}>
        <DialogContent className="sm:max-w-md shadow-glass bg-glass border-none backdrop-blur-md">
          <DialogHeader>
            <DialogTitle className="text-normal flex items-center gap-2">
              <FolderPlusIcon className="w-5 h-5" />
              Add Local Workspace
            </DialogTitle>
            <DialogDescription className="text-low">
              Select a local folder to add it as a new Vibe Kanban workspace.
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col items-center justify-center p-6 space-y-4">
               <Button 
                 onClick={handleSelectFolder} 
                 disabled={isCreating}
                 className="w-full flex items-center justify-center gap-2 h-12 shadow-glass bg-primary/20 hover:bg-primary/30 border border-secondary"
               >
                 <FolderOpenIcon className="w-5 h-5" />
                 {isCreating ? 'Adding...' : 'Select Local Directory'}
               </Button>
               {error && <p className="text-red-400 text-sm">{error}</p>}
          </div>
        </DialogContent>
      </Dialog>
    );
  }
);

export const CreateRemoteProjectDialog = defineModal<
  CreateRemoteProjectDialogProps,
  CreateRemoteProjectResult
>(CreateRemoteProjectDialogImpl);
