import { useQuery } from '@tanstack/vue-query'
import { computed } from 'vue'

import { get_default_user, users } from '@/helpers/auth.js'

export function useClientAccount() {
	const query = useQuery({
		queryKey: ['minecraft-account', 'selected'],
		networkMode: 'always',
		queryFn: async () => {
			const [id, accounts] = await Promise.all([get_default_user(), users()])
			return (
				(accounts as { profile: { id: string; name: string } }[]).find(
					(account) => account.profile.id === id,
				)?.profile ?? null
			)
		},
	})
	return { account: computed(() => query.data.value ?? null), loading: query.isPending }
}
