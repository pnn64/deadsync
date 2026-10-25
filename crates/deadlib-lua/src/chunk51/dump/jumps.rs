//! Batch insertion of guarded relays for native signed-18-bit branches.
use super::fail;
use crate::Error;
use crate::bytecode::Prototype;

const LIMIT: i64 = 131071;
const STEP: usize = 65536;
const JMP: u32 = 22;

#[derive(Clone, Copy)]
struct Edge {
    site: usize,
    target: usize,
    origin: usize,
}
struct Request {
    edge: usize,
    cut: usize,
    target: usize,
    origin: usize,
}
struct Relay {
    cut: usize,
    target: usize,
    site: usize,
    origin: usize,
    context: usize,
}

struct Site {
    pc: usize,
    origin: usize,
    relay: bool,
}
pub(super) struct LocalRange {
    pub index: usize,
    pub start: u32,
    pub end: u32,
}

fn offset(edge: Edge) -> i64 {
    edge.target as i64 - edge.site as i64 - 1
}
fn encode(word: u32, distance: i64) -> Result<u32, Error> {
    if !(-LIMIT..=LIMIT).contains(&distance) {
        return Err(fail());
    }
    Ok((word & 0x3fff) | (((distance + LIMIT) as u32) << 14))
}

pub(super) fn relax(
    code: &mut Vec<u32>,
    lines: &mut Vec<u32>,
    pcs: &mut [u32],
    patches: &[(usize, usize)],
    prototype: &Prototype,
) -> Result<Vec<LocalRange>, Error> {
    let mut safe = vec![false; code.len() + 1];
    for &pc in pcs.iter() {
        if pc != 0 && (pc as usize) < code.len() {
            safe[pc as usize] = true;
        }
    }
    let mut edges = Vec::with_capacity(patches.len());
    let mut patch = 0;
    let mut pc = 0;
    while pc < code.len() {
        let word = code[pc];
        let op = word & 63;
        // Open results must be immediately followed by an open consumer;
        // the Lua verifier checks that adjacency, not just the control flow.
        if (matches!(op, 28 | 29) && (word >> 14) & 511 == 0) || (op == 37 && word >> 23 == 0) {
            safe[pc + 1] = false;
        }
        if matches!(op, 22 | 31 | 32) {
            let target = if patches.get(patch).is_some_and(|&(site, _)| site == pc) {
                let target = *pcs.get(patches[patch].1).ok_or_else(fail)? as usize;
                patch += 1;
                target
            } else {
                let target = pc as i64 + 1 + i64::from(word >> 14) - LIMIT;
                usize::try_from(target).map_err(|_| fail())?
            };
            if target >= code.len() {
                return Err(fail());
            }
            let origin = pcs.partition_point(|&p| p as usize <= pc).saturating_sub(1);
            edges.push(Edge {
                site: pc,
                target,
                origin,
            });
        }
        pc += if op == 34 && (word >> 14) & 511 == 0 {
            2
        } else {
            1
        };
    }
    if patch != patches.len() {
        return Err(fail());
    }
    let mut sites: Vec<Site> = Vec::new();
    loop {
        let cuts: Vec<_> = safe
            .iter()
            .enumerate()
            .filter_map(|(pc, &yes)| yes.then_some(pc))
            .collect();
        let mut requests = Vec::new();
        for (index, &edge) in edges.iter().enumerate() {
            let distance = offset(edge);
            if (-LIMIT..=LIMIT).contains(&distance) {
                continue;
            }
            let cut = if distance > 0 {
                let wanted = edge.site + STEP;
                let index = cuts
                    .partition_point(|&cut| cut <= wanted)
                    .checked_sub(1)
                    .ok_or_else(fail)?;
                let cut = cuts[index];
                if cut <= edge.site || cut >= edge.target {
                    return Err(fail());
                }
                cut
            } else {
                let wanted = edge.site.saturating_sub(STEP);
                let index = cuts.partition_point(|&cut| cut < wanted);
                let cut = *cuts.get(index).ok_or_else(fail)?;
                if cut >= edge.site || cut <= edge.target {
                    return Err(fail());
                }
                cut
            };
            requests.push(Request {
                edge: index,
                cut,
                target: edge.target,
                origin: edge.origin,
            });
        }
        if requests.is_empty() {
            for edge in edges {
                code[edge.site] = encode(code[edge.site], offset(edge))?;
            }
            return Ok(scopes(prototype, pcs, &sites));
        }
        requests.sort_unstable_by_key(|r| (r.cut, r.target, r.origin));
        let mut relays: Vec<Relay> = Vec::with_capacity(requests.len());
        let mut redirects = vec![usize::MAX; edges.len()];
        for request in requests {
            if relays.last().is_none_or(|r| {
                r.cut != request.cut || r.target != request.target || r.origin != request.origin
            }) {
                let context = sites
                    .binary_search_by_key(&request.cut, |s| s.pc)
                    .map_or_else(
                        |_| {
                            pcs.partition_point(|&p| p as usize <= request.cut)
                                .saturating_sub(1)
                        },
                        |index| sites[index].origin,
                    );
                relays.push(Relay {
                    cut: request.cut,
                    target: request.target,
                    site: 0,
                    origin: request.origin,
                    context,
                });
            }
            redirects[request.edge] = relays.len() - 1;
        }
        let new_len = code
            .len()
            .checked_add(relays.len().checked_mul(2).ok_or_else(fail)?)
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or_else(fail)?;
        let mut next = Vec::with_capacity(new_len);
        let mut next_lines = Vec::with_capacity(new_len);
        let mut next_safe = vec![false; new_len + 1];
        let mut positions = Vec::with_capacity(code.len() + 1);
        let mut relay = 0;
        for (pc, (&word, &line)) in code.iter().zip(lines.iter()).enumerate() {
            while relay < relays.len() && relays[relay].cut == pc {
                // Ordinary fallthrough skips the relay. Only its redirected
                // branch enters it; neither instruction touches Lua registers.
                next_safe[next.len()] = true;
                next.push(JMP);
                next_lines.push(line);
                relays[relay].site = next.len();
                next.push(JMP);
                next_lines.push(prototype.lines[relays[relay].origin]);
                relay += 1;
            }
            positions.push(next.len());
            next_safe[next.len()] = safe[pc];
            next.push(word);
            next_lines.push(line);
        }
        positions.push(next.len());
        for (index, edge) in edges.iter_mut().enumerate() {
            edge.site = positions[edge.site];
            edge.target = if redirects[index] == usize::MAX {
                positions[edge.target]
            } else {
                relays[redirects[index]].site
            };
        }
        for site in &mut sites {
            site.pc = positions[site.pc];
        }
        for relay in relays {
            edges.push(Edge {
                site: relay.site - 1,
                target: relay.site + 1,
                origin: relay.context,
            });
            edges.push(Edge {
                site: relay.site,
                target: positions[relay.target],
                origin: relay.origin,
            });
            sites.push(Site {
                pc: relay.site - 1,
                origin: relay.context,
                relay: false,
            });
            sites.push(Site {
                pc: relay.site,
                origin: relay.origin,
                relay: true,
            });
        }
        sites.sort_unstable_by_key(|s| s.pc);
        for pc in pcs.iter_mut() {
            *pc = positions[*pc as usize] as u32;
        }
        *code = next;
        *lines = next_lines;
        safe = next_safe;
    }
}

fn scopes(prototype: &Prototype, pcs: &[u32], sites: &[Site]) -> Vec<LocalRange> {
    let relays: Vec<_> = sites.iter().filter(|s| s.relay).collect();
    let mut ranges = Vec::with_capacity(prototype.debug.locals.len());
    for (index, local) in prototype.debug.locals.iter().enumerate() {
        let mut start = pcs[local.start as usize];
        let end = pcs[local.end as usize];
        let first = relays.partition_point(|s| (s.pc as u32) < start);
        for relay in &relays[first..] {
            let pc = relay.pc as u32;
            if pc >= end {
                break;
            }
            if start < pc {
                ranges.push(LocalRange {
                    index,
                    start,
                    end: pc,
                });
            }
            start = pc + 1;
        }
        if start < end {
            ranges.push(LocalRange { index, start, end });
        }
    }
    // Relay execution inherits its branch's scope, not the scope of the
    // physically adjacent body instructions. Sweep source scopes once.
    let mut by_origin = relays;
    by_origin.sort_unstable_by_key(|s| s.origin);
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0;
    for relay in by_origin {
        active.retain(|&i| relay.origin < prototype.debug.locals[i].end as usize);
        while next < prototype.debug.locals.len()
            && prototype.debug.locals[next].start as usize <= relay.origin
        {
            if relay.origin < prototype.debug.locals[next].end as usize {
                active.push(next);
            }
            next += 1;
        }
        for &index in &active {
            ranges.push(LocalRange {
                index,
                start: relay.pc as u32,
                end: relay.pc as u32 + 1,
            });
        }
    }
    ranges.sort_unstable_by_key(|r| (r.start, r.index));
    ranges
}
