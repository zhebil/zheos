use crate::memory::{
    pfn::Pfn,
    region::{PageRange, Region},
};

/// The parts of `arena` that none of `taken` covers, in address order.
///
/// The arena rounds inward and every taken region rounds outward, so a run is
/// never half a page, and a region starting mid-page claims the whole page.
pub fn gaps<I>(arena: Region, taken: I) -> impl Iterator<Item = PageRange>
where
    I: Iterator<Item = Region> + Clone,
{
    let mut cursor = Pfn::from_addr_up(arena.base);
    let end = Pfn::from_addr_down(arena.end());

    core::iter::from_fn(move || {
        while let Some(stop) = containing(arena, taken.clone(), cursor) {
            cursor = stop;
        }

        if cursor >= end {
            return None;
        }

        let run = PageRange {
            start: cursor,
            end: next_base_above(arena, taken.clone(), cursor)
                .unwrap_or(end)
                .min(end),
        };

        cursor = run.end;

        Some(run)
    })
}

fn ranges<I>(arena: Region, taken: I) -> impl Iterator<Item = PageRange>
where
    I: Iterator<Item = Region>,
{
    taken.filter_map(move |region| PageRange::new(region, arena))
}

/// The end of the run covering `pfn`, if one does.
fn containing<I>(arena: Region, taken: I, pfn: Pfn) -> Option<Pfn>
where
    I: Iterator<Item = Region>,
{
    ranges(arena, taken)
        .find(|range| range.contains(pfn))
        .map(|range| range.end)
}

/// The nearest run that starts after `pfn`.
fn next_base_above<I>(arena: Region, taken: I, pfn: Pfn) -> Option<Pfn>
where
    I: Iterator<Item = Region>,
{
    ranges(arena, taken)
        .filter(|range| range.start > pfn)
        .map(|range| range.start)
        .min()
}
