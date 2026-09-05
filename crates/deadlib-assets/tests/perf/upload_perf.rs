use super::*;
use std::hint::black_box;

#[test]
#[ignore = "manual release benchmark"]
fn hot_path_bench() {
    let budget = TextureUploadBudget {
        max_uploads: 8,
        max_bytes: usize::MAX,
    };
    let mut queue = TextureUploadQueue::default();
    let mut image = Some(RgbaImage::new(64, 64));
    crate::perf::measure("owned_upload_queue", 1, || {
        queue.push_owned(black_box(1), image.take().unwrap(), SamplerDesc::default());
        let (_, mut upload) = queue.pop_next(budget, 0, 0).unwrap();
        black_box(upload.image());
        // Recover the caller's pixel buffer; measure queue ownership overhead,
        // excluding decode, pixel-buffer creation, and GPU work in both builds.
        image = Some(match upload.image.take().unwrap() {
            UploadImage::Shared(shared) => Arc::try_unwrap(shared).unwrap(),
            UploadImage::Owned(owned) => owned,
            _ => unreachable!(),
        });
    });
}

#[test]
fn owned_upload_keeps_pixels_and_has_no_warm_queue_churn() {
    let budget = TextureUploadBudget {
        max_uploads: 1,
        max_bytes: 32,
    };
    let mut queue = TextureUploadQueue::default();
    let mut source = Some(RgbaImage::from_pixel(3, 2, image::Rgba([7, 11, 19, 23])));
    let original = source.as_ref().unwrap().as_ptr();
    let mut cycle = || {
        queue.push_owned(7, source.take().unwrap(), SamplerDesc::default());
        let (handle, mut upload) = queue.pop_next(budget, 0, 0).unwrap();
        assert_eq!(handle, 7);
        assert_eq!(upload.bytes, 24);
        let UploadImage::Owned(image) = upload.image.take().unwrap() else {
            panic!("owned pixels must stay owned")
        };
        assert_eq!(image.as_ptr(), original);
        assert_eq!(image.get_pixel(2, 1).0, [7, 11, 19, 23]);
        source = Some(image);
    };
    cycle();
    crate::perf::assert_no_churn(|| {
        for _ in 0..100 {
            cycle();
        }
    });
}

#[test]
fn owned_replacement_preserves_order_budget_and_shared_data() {
    let mut queue = TextureUploadQueue::default();
    let shared = Arc::new(RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 4])));
    let sampler = SamplerDesc {
        filter: deadlib_render_core::SamplerFilter::Nearest,
        ..Default::default()
    };
    queue.push_owned(7, RgbaImage::new(1, 1), SamplerDesc::default());
    queue.push(11, Arc::clone(&shared), SamplerDesc::default());
    queue.push_owned(
        7,
        RgbaImage::from_pixel(8, 8, image::Rgba([5, 6, 7, 8])),
        sampler,
    );
    let budget = TextureUploadBudget {
        max_uploads: 2,
        max_bytes: 32,
    };
    let (handle, upload) = queue.pop_next(budget, 0, 0).unwrap();
    assert_eq!((handle, upload.bytes, upload.sampler), (7, 256, sampler));
    assert!(queue.pop_next(budget, 1, upload.bytes).is_none());
    let (handle, upload) = queue.pop_next(budget, 0, 0).unwrap();
    assert_eq!(handle, 11);
    let TextureUploadImage::Rgba(image) = upload.image() else {
        panic!("RGBA expected")
    };
    assert_eq!(image.as_ptr(), shared.as_ptr());
    assert_eq!(image.get_pixel(0, 0).0, [1, 2, 3, 4]);
    assert!(queue.pop_next(budget, 0, 0).is_none());
}
