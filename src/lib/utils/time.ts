export function formatRelativeTime(dateStr: string): string {
	const date = new Date(dateStr);
	const now = new Date();
	const diff = now.getTime() - date.getTime();
	const seconds = Math.floor(diff / 1000);
	const minutes = Math.floor(seconds / 60);
	const hours = Math.floor(minutes / 60);
	const days = Math.floor(hours / 24);

	if (seconds < 60) return 'just now';
	if (minutes < 60) return `${minutes}m ago`;
	if (hours < 24) return `${hours}h ago`;
	if (days < 7) return `${days}d ago`;

	return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

export function formatDate(dateStr: string): string {
	return new Date(dateStr).toLocaleDateString(undefined, {
		year: 'numeric',
		month: 'short',
		day: 'numeric',
		hour: '2-digit',
		minute: '2-digit'
	});
}

// Relative-date bucket label for grouping the notes list:
// Today / Yesterday / Previous 7 Days / Previous 30 Days / "June" (this year) / "June 2025".
// Buckets are contiguous, so a list already sorted by date groups by consecutive label.
export function dateBucketLabel(dateStr: string, now: Date = new Date()): string {
	const d = new Date(dateStr);
	const dayMs = 86400000;
	const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
	const today = startOfDay(now);
	const day = startOfDay(d);
	if (day >= today) return 'Today';
	if (day >= today - dayMs) return 'Yesterday';
	if (day >= today - 7 * dayMs) return 'Previous 7 Days';
	if (day >= today - 30 * dayMs) return 'Previous 30 Days';
	if (d.getFullYear() === now.getFullYear()) return d.toLocaleDateString(undefined, { month: 'long' });
	return d.toLocaleDateString(undefined, { month: 'long', year: 'numeric' });
}
