function pathParts(path: string): string[] {
	return path.replace(/\\/g, '/').split('/').filter(Boolean);
}

function pathRoot(path: string): string {
	const normalized = path.replace(/\\/g, '/');
	const drive = normalized.match(/^([A-Za-z]:)\//);
	if (drive) return drive[1].toLowerCase();
	return normalized.startsWith('/') ? '/' : '';
}

export function relativePath(fromDirectory: string, targetPath: string): string {
	const from = pathParts(fromDirectory);
	const target = pathParts(targetPath);
	if (pathRoot(fromDirectory) !== pathRoot(targetPath)) return targetPath.replace(/\\/g, '/');
	const windowsPath = /^[A-Za-z]:[\\/]/.test(fromDirectory) || /^[A-Za-z]:[\\/]/.test(targetPath);
	const equal = windowsPath
		? (left: string, right: string) => left.toLowerCase() === right.toLowerCase()
		: (left: string, right: string) => left === right;

	let common = 0;
	while (common < from.length && common < target.length && equal(from[common], target[common])) {
		common++;
	}

	const result = [...Array(from.length - common).fill('..'), ...target.slice(common)];
	return result.join('/') || '.';
}

export function resolvePathFromFile(filePath: string, targetPath: string): string {
	const normalizedFile = filePath.replace(/\\/g, '/');
	const normalizedTarget = targetPath.replace(/\\/g, '/');
	const lastSeparator = normalizedFile.lastIndexOf('/');
	const fileDirectory = lastSeparator >= 0 ? normalizedFile.slice(0, lastSeparator) : '';
	const combined = normalizedTarget.startsWith('/') || /^[A-Za-z]:\//.test(normalizedTarget)
		? normalizedTarget
		: `${fileDirectory}/${normalizedTarget}`;
	const drive = combined.match(/^([A-Za-z]:)\//);
	const unc = combined.startsWith('//');
	const absolute = !unc && combined.startsWith('/');
	let prefix = '';
	let remainder = combined;
	if (drive) {
		prefix = `${drive[1]}/`;
		remainder = combined.slice(drive[0].length);
	} else if (unc) {
		prefix = '//';
		remainder = combined.slice(2);
	} else if (absolute) {
		prefix = '/';
		remainder = combined.slice(1);
	}
	const resolved: string[] = [];

	for (const segment of remainder.split('/')) {
		if (!segment || segment === '.') continue;
		if (segment === '..') {
			resolved.pop();
		} else {
			resolved.push(segment);
		}
	}

	return prefix + resolved.join('/');
}
