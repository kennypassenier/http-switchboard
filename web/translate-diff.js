// The before/after comparison on the "Try a translation" page. The package ships the style of `.kp-diff`, not the lines,
// so this module computes a line diff (longest common subsequence, indentation ignored when matching) and draws it.

/**
 * A line diff of two texts, indentation ignored when matching so a re-nested line still counts as the same line.
 * @param {string[]} a @param {string[]} b
 */
function lineOps(a, b) {
    const same = (/** @type {number} */ i, /** @type {number} */ j) => a[i].trim() === b[j].trim();
    const width = b.length + 1;
    const table = new Uint32Array((a.length + 1) * width);
    for (let i = a.length - 1; i >= 0; i--) {
        for (let j = b.length - 1; j >= 0; j--) {
            table[i * width + j] = same(i, j) ? table[(i + 1) * width + j + 1] + 1 : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
        }
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
        } else if (j < b.length && (i === a.length || table[i * width + j + 1] >= table[(i + 1) * width + j])) {
            ops.push({ kind: 'added', text: b[j], a: i + 1, b: j + 1 });
            j++;
        } else {
            ops.push({ kind: 'removed', text: a[i], a: i + 1, b: j + 1 });
            i++;
        }
    }
    return ops;
}

/** The most cells (lines before x lines after) the comparison builds; 4 million is a few tens of MB and well under a second. */
const MAX_CELLS = 4_000_000;
const TOO_LARGE = 'Too large to compare line by line.';

/** What the folded box shows once opened: the texts to compare, or null when there is nothing to show. @type {{ before: string, after: string } | null} */
let pending = null;
/** @type {{ box: HTMLDetailsElement, diff: HTMLElement, diffCount: HTMLElement } | null} */
let target = null;

/** Draw the pending comparison into the target, when the box is open. */
function drawPending() {
    if (!pending || !target || !target.box.open) return;
    const { diff, diffCount } = target;
    const { before, after } = pending;
    pending = null;
    const a = before.split('\n');
    const b = after.split('\n');
    if ((a.length + 1) * (b.length + 1) > MAX_CELLS) {
        diff.replaceChildren();
        diff.setAttribute('aria-label', TOO_LARGE);
        diffCount.textContent = '';
        const note = document.createElement('span');
        note.className = 'kp-diff__hunk';
        note.textContent = `${TOO_LARGE} Compare the two bodies above by eye.`;
        diff.append(note);
        return;
    }
    drawOps(diff, diffCount, lineOps(a, b));
}

/**
 * Queue the comparison for the folded "Show line diff": it is computed only when the box is open (now, or when the reader
 * opens it), so a large message never costs time nobody asked for.
 * @param {HTMLDetailsElement} box @param {HTMLElement} diff the `.kp-diff` element @param {HTMLElement} diffCount the counts line
 * @param {string} before @param {string} after
 */
export function queueDiff(box, diff, diffCount, before, after) {
    if (!target) box.addEventListener('toggle', drawPending);
    target = { box, diff, diffCount };
    pending = { before, after };
    diff.replaceChildren();
    diffCount.textContent = '';
    drawPending();
}

/** Forget the queued comparison (the fields card was cleared). */
export function clearDiff() {
    pending = null;
}

/**
 * Draw `.kp-diff` with every line (the approved default context, "All lines": one block).
 * @param {HTMLElement} diff @param {HTMLElement} diffCount @param {ReturnType<typeof lineOps>} ops
 */
function drawOps(diff, diffCount, ops) {
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
