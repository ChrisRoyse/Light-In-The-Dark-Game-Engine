const INF: f32 = 1.0e20;

pub(crate) fn sdf_from_coverage(
    coverage: &[u8],
    width: usize,
    height: usize,
    spread_px: f32,
) -> Vec<u8> {
    let inside = coverage
        .iter()
        .map(|coverage| *coverage >= 128)
        .collect::<Vec<_>>();
    let dist_to_inside = distance_to(&inside, width, height, true);
    let dist_to_outside = distance_to(&inside, width, height, false);
    let spread = spread_px.max(1.0);
    inside
        .iter()
        .enumerate()
        .map(|(index, is_inside)| {
            let signed = if *is_inside {
                dist_to_outside[index].sqrt()
            } else {
                -dist_to_inside[index].sqrt()
            };
            (128.0 + signed * 127.0 / spread).round().clamp(0.0, 255.0) as u8
        })
        .collect()
}

fn distance_to(inside: &[bool], width: usize, height: usize, target_inside: bool) -> Vec<f32> {
    let mut f = vec![INF; width * height];
    for (index, value) in inside.iter().enumerate() {
        if *value == target_inside {
            f[index] = 0.0;
        }
    }
    distance_transform_2d(&f, width, height)
}

fn distance_transform_2d(f: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut tmp = vec![0.0; width * height];
    for x in 0..width {
        let mut column = vec![0.0; height];
        for y in 0..height {
            column[y] = f[y * width + x];
        }
        let transformed = distance_transform_1d(&column);
        for y in 0..height {
            tmp[y * width + x] = transformed[y];
        }
    }
    let mut out = vec![0.0; width * height];
    for y in 0..height {
        let start = y * width;
        let transformed = distance_transform_1d(&tmp[start..start + width]);
        out[start..start + width].copy_from_slice(&transformed);
    }
    out
}

fn distance_transform_1d(f: &[f32]) -> Vec<f32> {
    let n = f.len();
    if n == 0 {
        return Vec::new();
    }
    let mut v = vec![0_usize; n];
    let mut z = vec![0.0_f32; n + 1];
    let mut k = 0_usize;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;

    for q in 1..n {
        let mut s;
        loop {
            let p = v[k];
            s = intersection(f, q, p);
            if s > z[k] {
                break;
            }
            if k == 0 {
                break;
            }
            k -= 1;
        }
        if s <= z[k] {
            k = 0;
        } else {
            k += 1;
        }
        v[k] = q;
        z[k] = s;
        z[k + 1] = INF;
    }

    k = 0;
    let mut out = vec![0.0; n];
    for (q, out_value) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let d = q as f32 - v[k] as f32;
        *out_value = d * d + f[v[k]];
    }
    out
}

fn intersection(f: &[f32], q: usize, p: usize) -> f32 {
    let qf = q as f32;
    let pf = p as f32;
    ((f[q] + qf * qf) - (f[p] + pf * pf)) / (2.0 * (qf - pf))
}
