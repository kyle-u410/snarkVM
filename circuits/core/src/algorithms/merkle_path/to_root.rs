// Copyright (C) 2019-2022 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

use super::*;

use crate::traits::Hash;

impl<E: Environment, H: Hash> MerklePath<E, H>
where
    <<H as Hash>::Output as Ternary>::Boolean: From<Boolean<E>>,
    <H as Hash>::Output: From<<<H as Hash>::Output as Ternary>::Output>,
    Vec<<H as Hash>::Input>: From<<H as Hash>::Output>,
{
    pub fn to_root(&self, crh: &H, leaf: &[H::Input]) -> H::Output {
        let mut curr_hash = crh.hash(leaf);

        // To traverse up a MT, we iterate over the path from bottom to top

        // At any given bit, the bit being 0 indicates our currently hashed value is the left,
        // and the bit being 1 indicates our currently hashed value is on the right.
        // Thus `left_hash` is the sibling if bit is 1, and it's the computed hash if bit is 0
        for (bit, sibling) in self.traversal.iter().zip_eq(&self.path) {
            let left_hash: H::Output = H::Output::ternary(&bit.clone().into(), sibling, &curr_hash).into();
            let right_hash: H::Output = H::Output::ternary(&bit.clone().into(), &curr_hash, sibling).into();

            let left_input: Vec<H::Input> = left_hash.into();
            let right_input: Vec<H::Input> = right_hash.into();

            // TODO (raychu86): Handle issue with merkle tree inner node hashing.
            //  The native tree hashes bytes, whereas the circuit tree hashes bits, which means the padding is incorrectly done.
            //  Native input = left_hash bits + padding (to fill bytes) + right_hash bits + padding
            //  Circuit input = left_hash bits + right_hash bits + padding.

            curr_hash = crh.hash(&[left_input, right_input].concat());
        }

        curr_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algorithms::Pedersen;

    use snarkvm_algorithms::{
        crh::PedersenCompressedCRH as NativePedersen,
        merkle_tree::{MaskedMerkleTreeParameters, MerkleTree},
        traits::MerkleParameters,
    };

    use snarkvm_circuits_environment::{assert_scope, Circuit, Mode};
    use snarkvm_curves::{bls12_377::Fr, edwards_bls12::EdwardsProjective};
    use snarkvm_utilities::{test_rng, ToBits, UniformRand};

    use std::sync::Arc;

    const PEDERSEN_NUM_WINDOWS: usize = 256;
    const PEDERSEN_WINDOW_SIZE: usize = 4;
    const TREE_DEPTH: usize = 4;
    const MESSAGE: &str = "Pedersen merkle path test";

    type NativeH = NativePedersen<EdwardsProjective, PEDERSEN_NUM_WINDOWS, PEDERSEN_WINDOW_SIZE>;
    type Parameters = MaskedMerkleTreeParameters<NativeH, TREE_DEPTH>;

    type H = Pedersen<Circuit, PEDERSEN_NUM_WINDOWS, PEDERSEN_WINDOW_SIZE>;

    fn check_to_root(
        mode: Mode,
        use_bad_root: bool,
        num_inputs: usize,
        num_constants: usize,
        num_public: usize,
        num_private: usize,
        num_constraints: usize,
    ) {
        let merkle_tree_parameters = Parameters::setup(MESSAGE);
        let crh = H::setup(MESSAGE);

        let mut rng = test_rng();
        let mut leaves = Vec::new();
        for _ in 0..1 << Parameters::DEPTH {
            leaves.push(Fr::rand(&mut rng));
        }

        let merkle_tree = MerkleTree::new(Arc::new(merkle_tree_parameters), &leaves).unwrap();
        let root = merkle_tree.root();

        for (i, leaf) in leaves.iter().enumerate() {
            let proof = merkle_tree.generate_proof(i, &leaf).unwrap();
            assert!(proof.verify(root, &leaf).unwrap());

            let leaf_bits = leaf.to_bits_le();
            let root = if use_bad_root { Default::default() } else { *root };

            Circuit::scope(format!("Poseidon {mode} merkle tree {i}"), || {
                let traversal = proof.position_list().collect::<Vec<_>>();
                let path = proof.path.clone();
                let merkle_path = MerklePath::<Circuit, H>::new(mode, (traversal, path));

                let circuit_leaf = leaf_bits
                    .iter()
                    .map(|bit| <H as Hash>::Input::new(mode, *bit))
                    .collect::<Vec<<H as Hash>::Input>>();
                let candidate_root = merkle_path.to_root(&crh, &circuit_leaf);

                assert_eq!(*leaf.to_bits_le(), circuit_leaf.eject_value());
                assert_eq!(root, candidate_root.eject_value());

                let case = format!("(mode = {mode}, num_inputs = {num_inputs})");
                assert_scope!(case, num_constants, num_public, num_private, num_constraints);
            });
        }
    }

    #[test]
    fn test_good_root_constant() {
        check_to_root(Mode::Constant, false, 0, 2773, 0, 0, 0);
    }

    #[test]
    fn test_good_root_public() {
        check_to_root(Mode::Public, false, 0, 487, 9, 4005, 4018);
    }

    #[test]
    fn test_good_root_private() {
        check_to_root(Mode::Private, false, 0, 487, 0, 4014, 4018);
    }

    #[should_panic]
    #[test]
    fn test_bad_root_public() {
        check_to_root(Mode::Public, true, 0, 487, 9, 4005, 4018);
    }

    #[should_panic]
    #[test]
    fn test_bad_root_private() {
        check_to_root(Mode::Private, true, 0, 487, 0, 4014, 4018);
    }
}
