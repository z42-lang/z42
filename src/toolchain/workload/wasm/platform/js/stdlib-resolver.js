/** Create a synchronous resolver backed by exact logical file names. */
export function mapResolver(files) {
    return name => files.get(name) ?? null;
}

/** Read named SDK files from Node's local filesystem. */
export async function bundleStdlibNode() {
    const fs = await import('node:fs');
    const stdlib = new URL('./stdlib/', import.meta.url);
    return name => {
        try {
            return fs.readFileSync(namedFileUrl(name, stdlib));
        } catch (error) {
            if (error.code === 'ENOENT') return null;
            throw error;
        }
    };
}

/** The browser preloads the named files because the resolver is synchronous. */
export async function bundleStdlibBrowser(baseUrl) {
    const index = await fetch(new URL('files.json', baseUrl));
    if (!index.ok) throw new Error(`failed to read files.json: HTTP ${index.status}`);
    const names = await index.json();
    const entries = await Promise.all(names.map(async name => {
        const response = await fetch(namedFileUrl(name, baseUrl));
        if (!response.ok) throw new Error(`failed to read ${name}: HTTP ${response.status}`);
        return [name, new Uint8Array(await response.arrayBuffer())];
    }));
    return mapResolver(new Map(entries));
}

function namedFileUrl(name, baseUrl) {
    return new URL(name.split('/').map(encodeURIComponent).join('/'), baseUrl);
}
