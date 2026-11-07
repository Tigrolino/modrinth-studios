/**
 * Modrinth Studios addition: Kyros (node filesystem) emulation on top of the
 * Pterodactyl file manager API.
 */
import type { Kyros } from '@modrinth/api-client'

import { pteroBytes, pteroRequest, pteroWrite } from './bridge'
import type { PteroServerRef } from './ids'

export type PteroFileAttributes = {
	name: string
	mode: string
	size: number
	is_file: boolean
	is_symlink: boolean
	mimetype: string
	created_at: string
	modified_at: string
}

export function normalizePath(path: string | null | undefined): string {
	const parts = (path ?? '/').replace(/\\/g, '/').split('/').filter(Boolean)
	return `/${parts.join('/')}`
}

export function splitPath(path: string): { dir: string; name: string } {
	const normalized = normalizePath(path)
	const index = normalized.lastIndexOf('/')
	return {
		dir: index <= 0 ? '/' : normalized.slice(0, index),
		name: normalized.slice(index + 1),
	}
}

const toSeconds = (iso: string) => Math.floor(new Date(iso).getTime() / 1000) || 0

export async function listDir(
	ref: PteroServerRef,
	directory: string,
): Promise<PteroFileAttributes[]> {
	const response = await pteroRequest<{ data: { attributes: PteroFileAttributes }[] }>(
		ref.panelId,
		'GET',
		`/servers/${ref.identifier}/files/list?directory=${encodeURIComponent(normalizePath(directory))}`,
	)
	return response.data.map((d) => d.attributes)
}

export async function listDirSafe(
	ref: PteroServerRef,
	directory: string,
): Promise<PteroFileAttributes[]> {
	try {
		return await listDir(ref, directory)
	} catch {
		return []
	}
}

export async function listDirectory(
	ref: PteroServerRef,
	path: string,
	page: number,
	pageSize: number,
): Promise<Kyros.Files.v0.DirectoryResponse> {
	const directory = normalizePath(path)
	const entries = await listDir(ref, directory)
	entries.sort((a, b) =>
		a.is_file === b.is_file ? a.name.localeCompare(b.name) : a.is_file ? 1 : -1,
	)
	const items: Kyros.Files.v0.DirectoryItem[] = entries.map((entry) => ({
		name: entry.name,
		type: entry.is_symlink ? 'symlink' : entry.is_file ? 'file' : 'directory',
		path: directory === '/' ? `/${entry.name}` : `${directory}/${entry.name}`,
		modified: toSeconds(entry.modified_at),
		created: toSeconds(entry.created_at),
		size: entry.is_file ? entry.size : undefined,
	}))
	const size = Math.max(1, pageSize || items.length || 1)
	const total = Math.max(1, Math.ceil(items.length / size))
	const current = Math.min(Math.max(1, page || 1), total)
	return {
		items: items.slice((current - 1) * size, current * size),
		total,
		current,
	}
}

export async function createFolder(ref: PteroServerRef, path: string): Promise<void> {
	const { dir, name } = splitPath(path)
	await pteroRequest(ref.panelId, 'POST', `/servers/${ref.identifier}/files/create-folder`, {
		body: { root: dir, name },
	})
}

export async function writeText(ref: PteroServerRef, path: string, content: string) {
	await pteroRequest(
		ref.panelId,
		'POST',
		`/servers/${ref.identifier}/files/write?file=${encodeURIComponent(normalizePath(path))}`,
		{ text: content },
	)
}

export async function writeBlob(ref: PteroServerRef, path: string, blob: Blob) {
	const buffer = await blob.arrayBuffer()
	// Small files go through the write endpoint (keeps exact path even for
	// dotfiles); anything big uses a signed Wings upload.
	await pteroWrite(
		ref.panelId,
		ref.identifier,
		normalizePath(path),
		buffer,
		buffer.byteLength < 4 * 1024 * 1024 ? 'write' : 'upload',
	)
}

export async function readText(ref: PteroServerRef, path: string): Promise<string | null> {
	try {
		return await pteroRequest<string>(
			ref.panelId,
			'GET',
			`/servers/${ref.identifier}/files/contents?file=${encodeURIComponent(normalizePath(path))}`,
		).then((body) => (typeof body === 'string' ? body : JSON.stringify(body, null, 2)))
	} catch {
		return null
	}
}

export async function readBlob(ref: PteroServerRef, path: string): Promise<Blob> {
	const bytes = await pteroBytes(
		ref.panelId,
		`/servers/${ref.identifier}/files/contents?file=${encodeURIComponent(normalizePath(path))}`,
	)
	return new Blob([bytes])
}

export async function moveFile(ref: PteroServerRef, source: string, destination: string) {
	const from = normalizePath(source)
	const to = normalizePath(destination)
	await pteroRequest(ref.panelId, 'PUT', `/servers/${ref.identifier}/files/rename`, {
		body: { root: '/', files: [{ from: from.slice(1), to: to.slice(1) }] },
	})
}

export async function deleteFiles(ref: PteroServerRef, paths: string[]) {
	const byDir = new Map<string, string[]>()
	for (const path of paths) {
		const { dir, name } = splitPath(path)
		byDir.set(dir, [...(byDir.get(dir) ?? []), name])
	}
	for (const [root, files] of byDir) {
		await pteroRequest(ref.panelId, 'POST', `/servers/${ref.identifier}/files/delete`, {
			body: { root, files },
		})
	}
}

export async function decompress(ref: PteroServerRef, path: string) {
	const { dir, name } = splitPath(path)
	await pteroRequest(ref.panelId, 'POST', `/servers/${ref.identifier}/files/decompress`, {
		body: { root: dir, file: name },
	})
}

export async function compress(
	ref: PteroServerRef,
	root: string,
	files: string[],
): Promise<PteroFileAttributes> {
	const response = await pteroRequest<{ attributes: PteroFileAttributes }>(
		ref.panelId,
		'POST',
		`/servers/${ref.identifier}/files/compress`,
		{ body: { root: normalizePath(root), files } },
	)
	return response.attributes
}

export async function signedDownloadUrl(ref: PteroServerRef, path: string): Promise<string> {
	const response = await pteroRequest<{ attributes: { url: string } }>(
		ref.panelId,
		'GET',
		`/servers/${ref.identifier}/files/download?file=${encodeURIComponent(normalizePath(path))}`,
	)
	return response.attributes.url
}

/** Pulls a remote URL straight onto the server (Wings downloads it). */
export async function pullRemoteFile(
	ref: PteroServerRef,
	url: string,
	directory: string,
	filename: string,
): Promise<void> {
	try {
		await pteroRequest(ref.panelId, 'POST', `/servers/${ref.identifier}/files/pull`, {
			body: { url, directory: normalizePath(directory), filename, foreground: true },
		})
	} catch {
		// Remote downloads can be disabled in Wings; fall back to fetching
		// the file here and uploading it.
		const response = await fetch(url)
		if (!response.ok) throw new Error(`Could not download ${filename} (HTTP ${response.status})`)
		await pteroWrite(
			ref.panelId,
			ref.identifier,
			`${normalizePath(directory)}/${filename}`,
			await response.arrayBuffer(),
			'upload',
		)
	}
}

export async function stat(
	ref: PteroServerRef,
	path: string,
): Promise<Kyros.Files.v1.FileStatResponse> {
	const normalized = normalizePath(path)
	const { dir, name } = splitPath(normalized)
	if (normalized === '/') {
		return {
			name: '',
			full_path: '/',
			size_bytes: 0,
			type: 'directory',
			mtime: new Date().toISOString(),
			ctime: new Date().toISOString(),
		}
	}
	const entry = (await listDir(ref, dir)).find((e) => e.name === name)
	if (!entry) throw Object.assign(new Error(`${normalized} does not exist`), { statusCode: 404 })
	return {
		name: entry.name,
		full_path: normalized,
		size_bytes: entry.size,
		type: entry.is_symlink ? 'symlink' : entry.is_file ? 'regular' : 'directory',
		mtime: entry.modified_at,
		ctime: entry.created_at,
	}
}
