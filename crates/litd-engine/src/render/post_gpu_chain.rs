use super::super::super::PostProcessPlan;
use super::{PostPipelines, bind_group, texture_extent};

pub(crate) struct RenderPostChainArgs<'a> {
    pub(crate) device: &'a wgpu::Device,
    pub(crate) encoder: &'a mut wgpu::CommandEncoder,
    pub(crate) bind_layout: &'a wgpu::BindGroupLayout,
    pub(crate) pipelines: &'a PostPipelines,
    pub(crate) source_view: &'a wgpu::TextureView,
    pub(crate) target_view: &'a wgpu::TextureView,
    pub(crate) settings: &'a wgpu::Buffer,
    pub(crate) grade: &'a wgpu::Buffer,
    pub(crate) plan: &'a PostProcessPlan,
    pub(crate) bloom_format: wgpu::TextureFormat,
}

pub(in super::super) fn render_post_chain(args: RenderPostChainArgs<'_>) -> Vec<String> {
    let RenderPostChainArgs {
        device,
        encoder,
        bind_layout,
        pipelines,
        source_view,
        target_view,
        settings,
        grade,
        plan,
        bloom_format,
    } = args;
    let mut labels = Vec::new();
    if plan.settings.bloom.enabled {
        let (_down_textures, down_views) = bloom_views(
            device,
            "litd-post-bloom-down",
            plan.width,
            plan.height,
            plan.settings.bloom.mip_count,
            bloom_format,
        );
        let prefilter_group = bind_group(
            device,
            bind_layout,
            source_view,
            source_view,
            settings,
            grade,
        );
        render_fullscreen_pass(
            encoder,
            &down_views[0],
            &pipelines.bloom_prefilter,
            &prefilter_group,
            "litd-post-bloom-prefilter-pass",
        );
        labels.push("litd-post-bloom-prefilter-pass".to_string());
        for mip in 1..plan.settings.bloom.mip_count {
            let source = &down_views[(mip - 1) as usize];
            let group = bind_group(device, bind_layout, source, source, settings, grade);
            let label = format!("litd-post-bloom-downsample-mip-{mip}");
            render_fullscreen_pass(
                encoder,
                &down_views[mip as usize],
                &pipelines.bloom_downsample,
                &group,
                &label,
            );
            labels.push(label);
        }
        let (_up_textures, up_views) = bloom_views(
            device,
            "litd-post-bloom-up",
            plan.width,
            plan.height,
            plan.settings.bloom.mip_count.saturating_sub(1),
            bloom_format,
        );
        let mut current_view = &down_views[(plan.settings.bloom.mip_count - 1) as usize];
        for mip in (1..plan.settings.bloom.mip_count).rev() {
            let target_index = (mip - 1) as usize;
            let base_view = &down_views[target_index];
            let group = bind_group(
                device,
                bind_layout,
                current_view,
                base_view,
                settings,
                grade,
            );
            let label = format!("litd-post-bloom-upsample-mip-{}", mip - 1);
            render_fullscreen_pass(
                encoder,
                &up_views[target_index],
                &pipelines.bloom_upsample,
                &group,
                &label,
            );
            labels.push(label);
            current_view = &up_views[target_index];
        }
        let final_group = bind_group(
            device,
            bind_layout,
            source_view,
            current_view,
            settings,
            grade,
        );
        render_fullscreen_pass(
            encoder,
            target_view,
            &pipelines.composite,
            &final_group,
            "litd-post-composite-pass",
        );
    } else {
        let final_group = bind_group(
            device,
            bind_layout,
            source_view,
            source_view,
            settings,
            grade,
        );
        render_fullscreen_pass(
            encoder,
            target_view,
            &pipelines.composite,
            &final_group,
            "litd-post-composite-pass",
        );
    }
    labels.push("litd-post-composite-pass".to_string());
    labels
}

fn bloom_views(
    device: &wgpu::Device,
    label_prefix: &'static str,
    width: u32,
    height: u32,
    count: u32,
    format: wgpu::TextureFormat,
) -> (Vec<wgpu::Texture>, Vec<wgpu::TextureView>) {
    let textures = (0..count)
        .map(|mip| {
            bloom_texture(
                device,
                &format!("{label_prefix}-{mip}"),
                mip_size(width, mip),
                mip_size(height, mip),
                format,
            )
        })
        .collect::<Vec<_>>();
    let views = textures
        .iter()
        .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()))
        .collect();
    (textures, views)
}

fn bloom_texture(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: texture_extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn render_fullscreen_pass(
    encoder: &mut wgpu::CommandEncoder,
    target_view: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    label: &str,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..3, 0..1);
}

fn mip_size(size: u32, mip: u32) -> u32 {
    (size >> mip).max(1)
}
