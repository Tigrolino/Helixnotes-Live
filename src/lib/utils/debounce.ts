export function debounce<T extends (...args: any[]) => any>(fn: T, ms: number): T {
	let timeout: ReturnType<typeof setTimeout>;
	return ((...args: any[]) => {
		clearTimeout(timeout);
		timeout = setTimeout(() => fn(...args), ms);
	}) as T;
}
