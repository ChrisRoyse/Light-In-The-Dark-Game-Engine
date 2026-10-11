use super::ProbeFixtureReport;

const QUANT_DIM: usize = 128;

pub(crate) struct ProbeFixture {
    pub gemm_m: usize,
    pub gemm_k: usize,
    pub gemm_n: usize,
    pub gemm_a: Vec<f32>,
    pub gemm_b: Vec<f32>,
    pub cosine_dim: usize,
    pub cosine_candidates: usize,
    pub cosine_query: Vec<f32>,
    pub cosine_matrix: Vec<f32>,
    pub topk_scores: Vec<f32>,
    pub topk_k: usize,
}

impl ProbeFixture {
    pub(crate) fn report(&self) -> ProbeFixtureReport {
        ProbeFixtureReport {
            gemm_shape_mkn: [self.gemm_m, self.gemm_k, self.gemm_n],
            cosine_dim: self.cosine_dim,
            cosine_candidates: self.cosine_candidates,
            topk_k: self.topk_k,
        }
    }
}

pub(crate) fn known_probe_fixture() -> ProbeFixture {
    ProbeFixture {
        gemm_m: 2,
        gemm_k: 3,
        gemm_n: 2,
        gemm_a: vec![1.0, 2.0, 3.0, 4.0, -1.0, 0.5],
        gemm_b: vec![2.0, -1.0, 0.25, 0.5, 1.5, -2.0],
        cosine_dim: 4,
        cosine_candidates: 3,
        cosine_query: vec![1.0, 0.5, -0.25, 0.75],
        cosine_matrix: vec![
            1.0, 0.5, -0.25, 0.75, -0.5, 1.0, 0.75, -0.25, -1.0, -0.5, 0.25, -0.75,
        ],
        topk_scores: vec![0.125, 0.875, -0.25, 0.875, 0.5],
        topk_k: 3,
    }
}

pub(crate) fn known_quant_vector() -> Vec<f32> {
    vec![1.0 / (QUANT_DIM as f32).sqrt(); QUANT_DIM]
}
