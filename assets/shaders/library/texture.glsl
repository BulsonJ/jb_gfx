#extension GL_EXT_nonuniform_qualifier: enable

layout (set = 0, binding = 0) uniform sampler2D bindlessTextures[];

vec4 SampleBindlessTexture(int handle, vec2 texCoords)
{
    vec4 result = vec4(0);
    if (handle > 0){
        result = texture(bindlessTextures[nonuniformEXT(handle - 1)], texCoords);
    }
    return result;
}