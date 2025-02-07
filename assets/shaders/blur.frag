#version 460

layout (location = 0) in vec2 inTexCoords;

layout (location = 0) out vec4 outFragColor;

layout (set = 0, binding = 0) uniform sampler2D bloomImage;

layout( push_constant ) uniform constants
{
    int horizontal;
} pushConstants;


void main()
{
    bool horizontal = pushConstants.horizontal == 1;
    float weight[5] = float[] (0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
    vec2 tex_offset = 1.0 / textureSize(bloomImage, 0); // gets size of single texel
    vec3 result = texture(bloomImage, inTexCoords).rgb * weight[0]; // current fragment's contribution
    if(horizontal)
    {
        for(int i = 1; i < 5; ++i)
        {
            result += texture(bloomImage, inTexCoords + vec2(tex_offset.x * i, 0.0)).rgb * weight[i];
            result += texture(bloomImage, inTexCoords - vec2(tex_offset.x * i, 0.0)).rgb * weight[i];
        }
    }
    else
    {
        for(int i = 1; i < 5; ++i)
        {
            result += texture(bloomImage, inTexCoords + vec2(0.0, tex_offset.y * i)).rgb * weight[i];
            result += texture(bloomImage, inTexCoords - vec2(0.0, tex_offset.y * i)).rgb * weight[i];
        }
    }
    outFragColor = vec4(result, 1.0);
}