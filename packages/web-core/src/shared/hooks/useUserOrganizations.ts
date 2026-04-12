import { useQuery } from '@tanstack/react-query';
import type { ListOrganizationsResponse } from 'shared/types';
import { organizationKeys } from '@/shared/hooks/organizationKeys';

/**
 * Local-first stub: returns a single hard-coded "Local Workspace" organisation.
 * This lets the rest of the codebase continue to work without a real backend.
 */
export function useUserOrganizations() {
  return useQuery<ListOrganizationsResponse>({
    queryKey: organizationKeys.userList(),
    queryFn: async (): Promise<ListOrganizationsResponse> => ({
      organizations: [
        {
          id: 'local-org',
          name: 'Local Workspace',
          slug: 'local',
          handle: 'local',
          is_personal: true,
          issue_prefix: 'LW',
          user_role: 'owner',
          created_at: '2024-01-01T00:00:00Z',
          updated_at: '2024-01-01T00:00:00Z',
        } as any, // `handle` is not in OrganizationWithRole but harmless at runtime
      ],
    }),
  });
}
