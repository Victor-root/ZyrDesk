// From the screen's image to the encoder's picture, on the graphics card.
//
// One triangle covers the viewport, which the engine sets to where the
// image goes in the picture. The luma pass writes the picture's Y plane,
// the chroma pass its interleaved U and V plane at half size, each
// sample left-sited like H.264 and HEVC expect: the average of two
// bilinear taps, which weighs the columns around it 1/4, 1/2, 1/4. The
// pointer is blended in before the colours are converted. The colour
// pass writes plain red, green and blue, for cards that cannot render
// into the encoder's own format.

Texture2D screen_image : register(t0);
Texture2D pointer_paint : register(t1);
Texture2D pointer_flip : register(t2);
SamplerState smooth : register(s0);
SamplerState sharp : register(s1);

cbuffer Drawing : register(b0)
{
    // Y, U and V from gamma-encoded red, green and blue: weights, then
    // the offset.
    float4 y_weights;
    float4 u_weights;
    float4 v_weights;
    // Where the pointer lies in the image: left, top, right, bottom,
    // from 0 to 1.
    float4 pointer_rect;
    // One pixel of the picture, in the image's coordinates.
    float2 luma_step;
    // Quarter turns of the screen, clockwise, as Windows reports them.
    uint rotation;
    uint pointer_shown;
};

struct Vertex
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

Vertex main_vs(uint id : SV_VertexID)
{
    Vertex vertex;
    float2 uv = float2((id << 1) & 2, id & 2);
    vertex.position = float4(uv * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
    vertex.uv = uv;
    return vertex;
}

// Where a point of the image, as the desktop shows it, lies in the
// captured texture, which a turned screen gives unturned.
float2 unturned(float2 uv)
{
    if (rotation == 1)
        return float2(uv.y, 1.0 - uv.x);
    if (rotation == 2)
        return float2(1.0 - uv.x, 1.0 - uv.y);
    if (rotation == 3)
        return float2(1.0 - uv.y, uv.x);
    return uv;
}

float3 image(float2 uv)
{
    float3 rgb = screen_image.SampleLevel(smooth, unturned(uv), 0).rgb;
    if (pointer_shown != 0 && all(uv >= pointer_rect.xy) && all(uv < pointer_rect.zw))
    {
        float2 at = (uv - pointer_rect.xy) / (pointer_rect.zw - pointer_rect.xy);
        float4 paint = pointer_paint.SampleLevel(sharp, at, 0);
        rgb = lerp(rgb, paint.rgb, paint.a);
        float4 flip = pointer_flip.SampleLevel(sharp, at, 0);
        if (flip.a > 0.5)
        {
            uint3 screen_bits = (uint3)round(saturate(rgb) * 255.0);
            uint3 flip_bits = (uint3)round(flip.rgb * 255.0);
            rgb = (float3)(screen_bits ^ flip_bits) / 255.0;
        }
    }
    return rgb;
}

float main_luma(Vertex vertex) : SV_Target
{
    float3 rgb = image(vertex.uv);
    return dot(rgb, y_weights.xyz) + y_weights.w;
}

float2 main_chroma(Vertex vertex) : SV_Target
{
    float3 rgb = (image(vertex.uv - float2(luma_step.x, 0.0)) + image(vertex.uv)) * 0.5;
    return float2(dot(rgb, u_weights.xyz) + u_weights.w, dot(rgb, v_weights.xyz) + v_weights.w);
}

float4 main_colour(Vertex vertex) : SV_Target
{
    return float4(image(vertex.uv), 1.0);
}
