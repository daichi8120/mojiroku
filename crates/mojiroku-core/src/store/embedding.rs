//! 話者声紋ベクトルの低レベル数値ユーティリティ（`store` から分離した pure 関数）。
//!
//! BLOB ⇔ f32 の相互変換と、cosine 照合に使う内積・L2 正規化平均。DB アクセスを含まないため
//! 単体テストが容易。話者照合の本体（leave-one-recording-out のクエリ等）は `store` に残す。

/// f32 ベクトルを little-endian BLOB へ。
pub(super) fn f32_to_blob(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

/// little-endian BLOB を f32 ベクトルへ。
pub(super) fn blob_to_f32(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// 内積（双方 L2 正規化済みなら cosine）。
pub(super) fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// ベクトル群の平均 → L2 正規化（人物重心）。空なら空。
/// （実装は和を取って正規化するが、L2 正規化で大きさが消えるため平均と同じ向きになる。）
pub(super) fn l2_mean(vecs: &[Vec<f32>]) -> Vec<f32> {
    let Some(first) = vecs.first() else {
        return Vec::new();
    };
    let dim = first.len();
    let mut acc = vec![0.0f32; dim];
    for v in vecs {
        for (a, x) in acc.iter_mut().zip(v) {
            *a += x;
        }
    }
    let norm = acc.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in acc.iter_mut() {
            *x /= norm;
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_roundtrip_preserves_values() {
        let v = vec![0.0f32, 1.0, -1.5, 3.14159, f32::MIN_POSITIVE];
        let back = blob_to_f32(&f32_to_blob(&v));
        assert_eq!(v, back);
        // 空ベクトルも往復で空。
        assert!(blob_to_f32(&f32_to_blob(&[])).is_empty());
    }

    #[test]
    fn blob_to_f32_ignores_trailing_partial_chunk() {
        // chunks_exact(4) は端数バイトを捨てる（現挙動の固定）。
        let mut bytes = f32_to_blob(&[2.0]);
        bytes.push(0xAB); // 余分な 1 バイト
        assert_eq!(blob_to_f32(&bytes), vec![2.0]);
    }

    #[test]
    fn dot_computes_inner_product() {
        assert_eq!(dot(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]), 32.0); // 4 + 10 + 18
        // 単位ベクトル同士の cosine。
        assert_eq!(dot(&[1.0, 0.0], &[1.0, 0.0]), 1.0);
        assert_eq!(dot(&[1.0, 0.0], &[0.0, 1.0]), 0.0);
    }

    #[test]
    fn dot_zips_to_shorter_on_dimension_mismatch() {
        // 現挙動: zip は短い方で止まる（store 側は呼び出し前に len 一致でフィルタする）。
        assert_eq!(dot(&[1.0, 2.0, 99.0], &[3.0, 4.0]), 11.0); // 3 + 8
    }

    #[test]
    fn l2_mean_normalizes_centroid() {
        // [3,0] と [0,0] の和 [3,0] を L2 正規化 → [1,0]。
        let c = l2_mean(&[vec![3.0, 0.0], vec![0.0, 0.0]]);
        assert_eq!(c, vec![1.0, 0.0]);
        // 単一ベクトルでも単位長へ正規化。
        assert_eq!(l2_mean(&[vec![0.0, 4.0]]), vec![0.0, 1.0]);
    }

    #[test]
    fn l2_mean_edge_cases() {
        // 空入力 → 空。
        assert!(l2_mean(&[]).is_empty());
        // 全ゼロ → norm=0 なので正規化せずゼロのまま（NaN を出さない）。
        assert_eq!(l2_mean(&[vec![0.0, 0.0, 0.0]]), vec![0.0, 0.0, 0.0]);
    }
}
