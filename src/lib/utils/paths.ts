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

export function normalizeLocalAssetPath(path: string): string {
	return path
		.replace(/\\/g, '/')
		.replace(/^\/([A-Za-z]:\/)/, '$1');
}

export function assetUrlToLocalPath(source: string): string | null {
	if (!source.startsWith('asset:') && !source.startsWith('http://asset.localhost') && !source.startsWith('https://asset.localhost')) {
		return null;
	}
	try {
		const path = normalizeLocalAssetPath(decodeURIComponent(new URL(source).pathname));
		// Tauri prepends a URL-path slash to POSIX and UNC paths.
		return path.startsWith('//') ? path.substring(1) : path;
	} catch {
		return null;
	}
}

export function resolveVaultFilePath(
	targetPath: string,
	notePath: string | null,
	vaultRoot: string | null,
): string {
	const target = normalizeLocalAssetPath(targetPath);
	if (target.startsWith('/') || /^[A-Za-z]:\//.test(target)) return target;

	const root = vaultRoot?.replace(/\\/g, '/').replace(/\/$/, '') ?? '';
	if (target.startsWith('.helixnotes/') && root) {
		return resolvePathFromFile(`${root}/.vault-root`, target);
	}
	if (notePath) return resolvePathFromFile(notePath, target);
	if (root) return resolvePathFromFile(`${root}/.vault-root`, target);
	return target;
}

export function assetSourceToMarkdown(
	source: string,
	notePath: string | null,
	vaultRoot: string | null,
): string {
	if (source.startsWith('blob:')) return '';
	if (source.startsWith('imgproxy:') || source.startsWith('http://imgproxy.localhost') || source.startsWith('https://imgproxy.localhost')) {
		try {
			return decodeURIComponent(new URL(source).pathname.substring(1));
		} catch {
			return source;
		}
	}
	const absolutePath = assetUrlToLocalPath(source);
	if (absolutePath === null) return source;

	const normalizedRoot = vaultRoot?.replace(/\\/g, '/').replace(/\/$/, '');
	if (!normalizedRoot || !absolutePath.startsWith(normalizedRoot + '/')) return absolutePath;
	const vaultRelative = absolutePath.substring(normalizedRoot.length + 1);
	if (vaultRelative.startsWith('.helixnotes/')) return vaultRelative;
	if (!notePath) return vaultRelative;
	const normalizedNotePath = notePath.replace(/\\/g, '/');
	const noteDirectory = normalizedNotePath.substring(0, normalizedNotePath.lastIndexOf('/'));
	return relativePath(noteDirectory, absolutePath);
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
