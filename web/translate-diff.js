// The before/after comparison on the "Try a translation" page. The package ships the style of `.kp-diff`, not the lines,
// so this module computes a line diff (longest common subsequence, indentation ignored when matching) and draws it.

/**
 * A line diff of two texts, indentation ignored when matching so a re-nested line still counts as the same line.
 * @param {string[]} a @param {string[]} b
 */
function lineOps(a, b) {
    const same = (/** @type {number} */ i, /** @type {number} */ j) => a[i].trim() === b[j].trim();
    const table = Array.from({ length: a.length + 1 }, () => new Array(b.length + 1).fill(0));
    for (let i = a.length - 1; i >= 0; i--) {
        for (let j = b.length - 1; j >= 0; j--) table[i][j] = same(i, j) ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
    }
    /** @type {{ kind: 'same' | 'removed' | 'added', text: string, a: number, b: number }[]} */
    const ops = [];
    let i = 0;
    let j = 0;
    while (i < a.length || j < b.length) {
        if (i < a.length && j < b.length && same(i, j)) {
            ops.push({ kind: 'same', text: b[j], a: i + 1, b: j + 1 });
            i++;
            j++;
        } else if (j < b.length && (i === a.length || table[i][j + 1] >= table[i + 1][j])) {
            ops.push({ kind: 'added', text: b[j], a: i + 1, b: j + 1 });
            j++;
        } else {
            ops.push({ kind: 'removed', text: a[i], a: i + 1, b: j + 1 });
            i++;
        }
    }
    return ops;
}

/**
 * Draw `.kp-diff` with every line (the approved default context, "All lines": one block).
 * @param {HTMLElement} diff the `.kp-diff` element @param {HTMLElement} diffCount the counts line
 * @param {string} before @param {string} after
 */
export function drawDiff(diff, diffCount, before, after) {
    const ops = lineOps(before.split('\n'), after.split('\n'));
    const removed = ops.filter((o) => o.kind !== 'added');
    const added = ops.filter((o) => o.kind !== 'removed');
    diff.replaceChildren();
    const header = document.createElement('div');
    header.className = 'kp-diff__hunk';
    header.append(`@@ -${removed[0]?.a ?? 0},${removed.length} +${added[0]?.b ?? 0},${added.length} @@ `);
    const where = document.createElement('span');
    where.className = 'kp-diff__hunk-context';
    where.textContent = 'received → delivered';
    header.append(where);
    diff.append(header);
    for (const op of ops) {
        const row = document.createElement('span');
        row.className = 'kp-diff__line';
        row.dataset.kind = op.kind;
        const number = document.createElement('span');
        number.className = 'kp-diff__number';
        number.textContent = String(op.kind === 'added' ? op.b : op.a);
        const sign = document.createElement('span');
        sign.className = 'kp-diff__sign';
        sign.textContent = op.kind === 'added' ? '+' : op.kind === 'removed' ? '-' : ' ';
        const text = document.createElement('span');
        text.textContent = op.text;
        row.append(number, sign, text);
        diff.append(row);
    }
    const counts = { removed: ops.filter((o) => o.kind === 'removed').length, added: ops.filter((o) => o.kind === 'added').length };
    diff.setAttribute('aria-label', `Changes from the received message to the delivered body: ${counts.removed} lines removed, ${counts.added} lines added`);
    diffCount.textContent = `${counts.removed} lines removed · ${counts.added} lines added`;
}
