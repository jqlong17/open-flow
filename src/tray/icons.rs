/// 生成 22×22 RGBA 图标像素数据
/// macOS 菜单栏图标建议 22pt，高 DPI 用 44px，这里用 22px 足够
const SIZE: u32 = 22;

/// 画一个抗锯齿圆（中心 cx,cy 半径 r，颜色 rgba）
fn draw_circle(pixels: &mut Vec<u8>, cx: f32, cy: f32, r: f32, color: [u8; 4]) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            // 简单 1px 抗锯齿
            let alpha = ((r + 0.5 - dist).clamp(0.0, 1.0) * color[3] as f32) as u8;
            if alpha > 0 {
                let idx = ((y * SIZE + x) * 4) as usize;
                // alpha 混合叠加
                let src_a = alpha as f32 / 255.0;
                pixels[idx]     = (color[0] as f32 * src_a + pixels[idx] as f32 * (1.0 - src_a)) as u8;
                pixels[idx + 1] = (color[1] as f32 * src_a + pixels[idx + 1] as f32 * (1.0 - src_a)) as u8;
                pixels[idx + 2] = (color[2] as f32 * src_a + pixels[idx + 2] as f32 * (1.0 - src_a)) as u8;
                pixels[idx + 3] = alpha.max(pixels[idx + 3]);
            }
        }
    }
}

/// 画麦克风形状（简化：竖矩形 + 底部弧）
fn draw_mic(pixels: &mut Vec<u8>, color: [u8; 4]) {
    let cx = SIZE as f32 / 2.0;
    // 麦克风主体：圆角矩形 —— 用一系列横线填充
    let body_x1 = cx - 3.0;
    let body_x2 = cx + 3.0;
    let body_y1 = 3.0f32;
    let body_y2 = 13.0f32;
    let r = 3.0f32;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            // 圆角矩形 inside check
            let in_rect = fx >= body_x1 && fx <= body_x2 && fy >= body_y1 && fy <= body_y2;
            let in_top_circle = {
                let dx = fx - cx;
                let dy = fy - (body_y1 + r);
                dx * dx + dy * dy <= r * r
            };
            let in_bot_circle = {
                let dx = fx - cx;
                let dy = fy - (body_y2 - r);
                dx * dx + dy * dy <= r * r
            };
            if in_rect || in_top_circle || in_bot_circle {
                let idx = ((y * SIZE + x) * 4) as usize;
                pixels[idx]     = color[0];
                pixels[idx + 1] = color[1];
                pixels[idx + 2] = color[2];
                pixels[idx + 3] = color[3];
            }
        }
    }
    // 底部弧（半圆）
    let arc_cx = cx;
    let arc_cy = body_y2;
    let arc_r = 5.0f32;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            if fy >= arc_cy {
                let dx = fx - arc_cx;
                let dy = fy - arc_cy;
                let dist = (dx * dx + dy * dy).sqrt();
                let on_arc = (dist - arc_r).abs() < 0.8;
                if on_arc {
                    let idx = ((y * SIZE + x) * 4) as usize;
                    pixels[idx]     = color[0];
                    pixels[idx + 1] = color[1];
                    pixels[idx + 2] = color[2];
                    pixels[idx + 3] = color[3];
                }
            }
        }
    }
    // 底部竖线
    let lx = cx as u32;
    for y in (body_y2 as u32)..(body_y2 as u32 + 4) {
        if y < SIZE {
            let idx = ((y * SIZE + lx) * 4) as usize;
            pixels[idx]     = color[0];
            pixels[idx + 1] = color[1];
            pixels[idx + 2] = color[2];
            pixels[idx + 3] = color[3];
        }
    }
}

/// 待机图标：灰色麦克风
pub fn icon_idle() -> (Vec<u8>, u32, u32) {
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
    draw_mic(&mut pixels, [180, 180, 180, 220]);
    (pixels, SIZE, SIZE)
}

/// 录音中图标：红色麦克风 + 右上角红色圆点
pub fn icon_recording() -> (Vec<u8>, u32, u32) {
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
    draw_mic(&mut pixels, [220, 50, 50, 255]);
    // 右上角红点（表示 "live"）
    draw_circle(&mut pixels, 17.5, 4.5, 3.0, [255, 60, 60, 255]);
    (pixels, SIZE, SIZE)
}

/// 转写中图标：橙色麦克风（处理中）
pub fn icon_processing() -> (Vec<u8>, u32, u32) {
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
    draw_mic(&mut pixels, [220, 140, 30, 255]);
    (pixels, SIZE, SIZE)
}

pub fn make_tray_icon(rgba: Vec<u8>, w: u32, h: u32) -> tray_icon::Icon {
    tray_icon::Icon::from_rgba(rgba, w, h).expect("创建 tray 图标失败")
}
