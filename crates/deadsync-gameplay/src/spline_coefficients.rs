use std::{fmt, sync::Arc};

pub type SongLuaSplinePointCoefficients = [[f32; 4]; 3];
pub type SongLuaSplineAxisPage = Arc<[[f32; 4]]>;
pub const SONG_LUA_SPLINE_PAGE_POINTS: usize = 16;

/// Baked curves share exact per-axis pages across the layer. The point index
/// selects one page and row directly, including when sampling hold tangents.
pub struct SongLuaSplinePages {
    len: usize,
    pages: Box<[[SongLuaSplineAxisPage; 3]]>,
}

impl SongLuaSplinePages {
    pub fn new(len: usize, pages: Box<[[SongLuaSplineAxisPage; 3]]>) -> Self {
        assert_eq!(pages.len(), len.div_ceil(SONG_LUA_SPLINE_PAGE_POINTS));
        for (index, page) in pages.iter().enumerate() {
            let points =
                (len - index * SONG_LUA_SPLINE_PAGE_POINTS).min(SONG_LUA_SPLINE_PAGE_POINTS);
            assert!(page.iter().all(|axis| axis.len() == points));
        }
        Self { len, pages }
    }

    pub fn pages(&self) -> &[[SongLuaSplineAxisPage; 3]] {
        &self.pages
    }
}

#[derive(Clone)]
pub enum SongLuaSplineCoefficients {
    Dense(Arc<[SongLuaSplinePointCoefficients]>),
    Paged(Arc<SongLuaSplinePages>),
}

impl SongLuaSplineCoefficients {
    pub fn view(&self) -> SongLuaSplineCoefficientView<'_> {
        match self {
            Self::Dense(points) => SongLuaSplineCoefficientView::Dense(points),
            Self::Paged(pages) => SongLuaSplineCoefficientView::Paged(pages),
        }
    }

    pub fn len(&self) -> usize {
        self.view().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = SongLuaSplinePointCoefficients> + '_ {
        self.view().iter()
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Dense(a), Self::Dense(b)) => Arc::ptr_eq(a, b),
            (Self::Paged(a), Self::Paged(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl From<Vec<SongLuaSplinePointCoefficients>> for SongLuaSplineCoefficients {
    fn from(points: Vec<SongLuaSplinePointCoefficients>) -> Self {
        Self::Dense(points.into())
    }
}

impl PartialEq for SongLuaSplineCoefficients {
    fn eq(&self, other: &Self) -> bool {
        self.view() == other.view()
    }
}

impl fmt::Debug for SongLuaSplineCoefficients {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.view().fmt(f)
    }
}

#[derive(Clone, Copy)]
pub enum SongLuaSplineCoefficientView<'a> {
    Dense(&'a [SongLuaSplinePointCoefficients]),
    Paged(&'a SongLuaSplinePages),
}

impl SongLuaSplineCoefficientView<'_> {
    pub fn len(self) -> usize {
        match self {
            Self::Dense(points) => points.len(),
            Self::Paged(pages) => pages.len,
        }
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub fn get(self, index: usize) -> Option<SongLuaSplinePointCoefficients> {
        match self {
            Self::Dense(points) => points.get(index).copied(),
            Self::Paged(pages) if index < pages.len => {
                let axes = &pages.pages[index / SONG_LUA_SPLINE_PAGE_POINTS];
                let row = index % SONG_LUA_SPLINE_PAGE_POINTS;
                Some([axes[0][row], axes[1][row], axes[2][row]])
            }
            Self::Paged(_) => None,
        }
    }
}

impl<'a> SongLuaSplineCoefficientView<'a> {
    pub fn iter(self) -> impl ExactSizeIterator<Item = SongLuaSplinePointCoefficients> + 'a {
        (0..self.len()).map(move |index| self.get(index).unwrap())
    }
}

impl PartialEq for SongLuaSplineCoefficientView<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Dense(a), Self::Dense(b)) => a == b,
            _ => self.iter().eq(other.iter()),
        }
    }
}

impl fmt::Debug for SongLuaSplineCoefficientView<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
