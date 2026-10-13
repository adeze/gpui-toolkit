"""Content assertions reject common-mode defects that pixel parity accepts."""
import unittest
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
from qa_chess_content import chess_content_errors
from qa_showcase_native_wasm import px_diff_ratio, edge_iou, block_corr

ROOT = Path(__file__).resolve().parents[2]


def board(content="pieces", scale=1):
    image = Image.new("RGB", (440, 440), (19, 31, 46))
    draw = ImageDraw.Draw(image)
    font = ImageFont.truetype(str(ROOT / "crates/assets/fonts/dejavu/DejaVuSans.ttf"), 30)
    ranks = {0: "♜♞♝♛♚♝♞♜", 1: "♟" * 8, 6: "♙" * 8, 7: "♖♘♗♕♔♗♘♖"}
    for row, pieces in ranks.items():
        for column, piece in enumerate(pieces):
            x, y = 42 + column * 50, 42 + row * 50
            fill, ink = ((248, 250, 252), (20, 28, 42)) if row < 2 else ((20, 28, 42), (248, 250, 252))
            draw.ellipse((x-20, y-20, x+20, y+20), fill=fill)
            if content == "tofu":
                draw.rectangle((x-6, y-10, x+6, y+10), outline=ink, width=2)
            elif content == "pieces":
                draw.text((x, y), piece, font=font, fill=ink, anchor="mm")
    return image.resize((round(440*scale), round(440*scale)), Image.Resampling.LANCZOS)


class ChessContentTests(unittest.TestCase):
    def test_same_broken_board_passes_parity_but_fails_content(self):
        image = board("tofu")
        self.assertEqual(px_diff_ratio(image, image), 0)
        self.assertEqual(edge_iou(image, image), 1)
        self.assertAlmostEqual(block_corr(image, image), 1)
        self.assertTrue(chess_content_errors(image))

    def test_absent_pieces_fail(self):
        self.assertTrue(chess_content_errors(board("empty")))

    def test_real_chess_glyphs_pass_at_multiple_scales(self):
        for scale in (1, 2, .85):
            with self.subTest(scale=scale):
                self.assertEqual(chess_content_errors(board(scale=scale)), [])

    def test_missing_board_fails(self):
        self.assertTrue(chess_content_errors(Image.new("RGB", (1200, 900), "black")))
