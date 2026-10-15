import unittest

from gpui_toolkit import mesh


def _rotation_dataset():
    scalars = []
    vectors = []
    for k in range(5):
        for j in range(5):
            for i in range(5):
                x, y = i * 0.25, j * 0.25
                scalars.append(x + y + k * 0.25)
                vectors.append([-y, x, 0.0])
    return mesh.structured(
        [5, 5, 5],
        spacing=(0.25, 0.25, 0.25),
        scalars={"s": scalars},
        vectors={"velocity": vectors},
    )


_NATIVE_MISSING = mesh.problem(_rotation_dataset()).startswith(
    "gpui_toolkit native extension"
)


@unittest.skipIf(_NATIVE_MISSING, "native extension not built")
class MeshTests(unittest.TestCase):
    def test_structured_summary(self):
        summary = mesh.run(_rotation_dataset(), [])
        self.assertEqual(summary["points"], 125)
        self.assertEqual(summary["cells"], 64)
        self.assertEqual(mesh.problem(_rotation_dataset()), "")

    def test_vorticity_of_solid_rotation_is_two(self):
        summary = mesh.run(
            _rotation_dataset(),
            [{"op": "vorticity_magnitude", "field": "velocity"}],
        )
        field = next(
            f for f in summary["fields"] if f["name"] == "vorticity_magnitude"
        )
        self.assertAlmostEqual(field["min"], 2.0, places=9)
        self.assertAlmostEqual(field["max"], 2.0, places=9)

    def test_slice_and_streamlines(self):
        summary = mesh.run(
            _rotation_dataset(),
            [mesh.slice_(normal=(0, 0, 1), point=(0, 0, 0.5))],
        )
        self.assertEqual(summary["cells"], 32)
        lines = mesh.streamlines(
            _rotation_dataset(), [(0.5, 0.1, 0.5)], step=0.05, max_steps=40
        )
        self.assertEqual(len(lines), 2)  # both directions per seed
        self.assertGreater(len(lines[0]), 10)

    def test_errors_are_reported(self):
        with self.assertRaises(ValueError):
            mesh.run(_rotation_dataset(), [{"op": "explode"}])
        bad = _rotation_dataset()
        bad["structured"]["dims"] = [0, 2, 2]
        self.assertNotEqual(mesh.problem(bad), "")


class MeshSpecTests(unittest.TestCase):
    def test_spec_shape_without_native(self):
        spec = _rotation_dataset()
        self.assertEqual(spec["structured"]["dims"], [5, 5, 5])
        self.assertEqual(len(spec["fields"]), 2)
        op = mesh.clip(normal=(1, 0, 0), keep="negative")
        self.assertEqual(op["op"], "clip")


if __name__ == "__main__":
    unittest.main()
