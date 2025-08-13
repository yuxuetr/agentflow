#!/bin/bash
# AgentFlow CLI Image Processing Workflows Tutorial
# Master image generation and understanding with advanced techniques

set -e

# Color output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m'

# Configuration
WORKFLOW_DIR="./tutorial_02_image_workflows"
PAUSE_BETWEEN_STEPS=2

echo -e "${BOLD}${MAGENTA}🎨 AgentFlow CLI - Image Workflows Tutorial${NC}"
echo "=============================================="
echo ""
echo -e "${CYAN}This tutorial explores advanced image processing workflows,${NC}"
echo -e "${CYAN}teaching you professional techniques for creative projects.${NC}"
echo ""

# Create output directory
mkdir -p "$WORKFLOW_DIR"
echo -e "${YELLOW}📁 Image outputs will be saved to: $WORKFLOW_DIR${NC}"
echo ""

# Helper functions
workflow_step() {
    local step_num="$1"
    local title="$2"
    echo ""
    echo -e "${BOLD}${BLUE}Workflow $step_num: $title${NC}"
    echo "$(printf '=%.0s' {1..60})"
}

run_image_command() {
    local description="$1"
    local command="$2"
    local expected_output="$3"
    
    echo -e "${CYAN}🎯 $description${NC}"
    echo -e "${YELLOW}Command:${NC} $command"
    echo ""
    
    if eval "$command"; then
        if [[ -n "$expected_output" && -f "$expected_output" ]]; then
            local size=$(stat -f%z "$expected_output" 2>/dev/null || stat -c%s "$expected_output" 2>/dev/null || echo "unknown")
            echo -e "${GREEN}✅ Success! Created: $expected_output (${size} bytes)${NC}"
        else
            echo -e "${GREEN}✅ Success!${NC}"
        fi
    else
        echo -e "${RED}❌ Failed. Check your API key and connection.${NC}"
        exit 1
    fi
    
    echo ""
    sleep $PAUSE_BETWEEN_STEPS
}

explain_technique() {
    echo -e "${CYAN}📚 Technique Explanation:${NC}"
    echo -e "${CYAN}$1${NC}"
    echo ""
}

show_results() {
    local file="$1"
    local description="$2"
    
    if [[ -f "$file" ]]; then
        echo -e "${YELLOW}📄 $description:${NC}"
        echo "$(head -n 5 "$file")"
        echo ""
    fi
}

# Pre-flight check
echo "🔍 Prerequisites Check"
echo "====================="

if [[ -z "$STEP_API_KEY" ]]; then
    echo -e "${RED}❌ STEP_API_KEY not set. Please set your API key first.${NC}"
    exit 1
fi

if ! command -v agentflow &> /dev/null; then
    echo -e "${RED}❌ AgentFlow CLI not found. Please install it first.${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Ready for image workflow mastery!${NC}"

# Workflow 1: Style Variations
workflow_step "1" "Style Variation Techniques"

explain_technique "Learn how to create different artistic styles from the same concept using specific prompting techniques and parameters."

BASE_CONCEPT="A wise old tree in a meadow"

run_image_command \
    "Create photorealistic version" \
    "agentflow image generate '$BASE_CONCEPT, photorealistic, high detail, professional photography' --model step-1x-medium --size 768x768 --steps 40 --cfg-scale 8.0 --seed 100 --output '$WORKFLOW_DIR/tree_photorealistic.png'" \
    "$WORKFLOW_DIR/tree_photorealistic.png"

run_image_command \
    "Create artistic painting version" \
    "agentflow image generate '$BASE_CONCEPT, oil painting, impressionist style, vibrant brushstrokes, Claude Monet inspired' --model step-1x-medium --size 768x768 --steps 40 --cfg-scale 7.5 --seed 100 --output '$WORKFLOW_DIR/tree_painting.png'" \
    "$WORKFLOW_DIR/tree_painting.png"

run_image_command \
    "Create minimalist line art version" \
    "agentflow image generate '$BASE_CONCEPT, minimalist line art, black and white, simple lines, zen aesthetic' --model step-1x-medium --size 768x768 --steps 30 --cfg-scale 6.0 --seed 100 --output '$WORKFLOW_DIR/tree_lineart.png'" \
    "$WORKFLOW_DIR/tree_lineart.png"

explain_technique "Notice how the same seed (100) ensures similar composition while style keywords completely change the appearance."

# Workflow 2: Parameter Impact Analysis
workflow_step "2" "Understanding Parameter Impact"

explain_technique "Explore how different parameters affect image quality and style. This teaches you to fine-tune for specific needs."

EXPERIMENT_PROMPT="A futuristic cityscape at night with neon lights"

run_image_command \
    "Low steps (fast generation)" \
    "agentflow image generate '$EXPERIMENT_PROMPT' --steps 15 --cfg-scale 7.0 --seed 42 --output '$WORKFLOW_DIR/city_lowsteps.png'" \
    "$WORKFLOW_DIR/city_lowsteps.png"

run_image_command \
    "High steps (quality generation)" \
    "agentflow image generate '$EXPERIMENT_PROMPT' --steps 50 --cfg-scale 7.0 --seed 42 --output '$WORKFLOW_DIR/city_highsteps.png'" \
    "$WORKFLOW_DIR/city_highsteps.png"

run_image_command \
    "Low CFG scale (loose interpretation)" \
    "agentflow image generate '$EXPERIMENT_PROMPT' --steps 30 --cfg-scale 3.0 --seed 42 --output '$WORKFLOW_DIR/city_lowcfg.png'" \
    "$WORKFLOW_DIR/city_lowcfg.png"

run_image_command \
    "High CFG scale (strict interpretation)" \
    "agentflow image generate '$EXPERIMENT_PROMPT' --steps 30 --cfg-scale 12.0 --seed 42 --output '$WORKFLOW_DIR/city_highcfg.png'" \
    "$WORKFLOW_DIR/city_highcfg.png"

explain_technique "Compare these images to understand:"
echo "  • Steps: Quality vs Speed trade-off"
echo "  • CFG Scale: Creative freedom vs Prompt adherence"
echo "  • Seed: Reproducibility for A/B testing"

# Workflow 3: Advanced Prompting Techniques
workflow_step "3" "Advanced Prompting Techniques"

explain_technique "Master sophisticated prompting for professional results. Learn weight keywords, negative space, and composition control."

run_image_command \
    "Using detailed descriptive prompting" \
    "agentflow image generate 'A majestic red dragon perched on ancient stone ruins, golden sunlight filtering through mist, detailed scales, piercing amber eyes, mystical atmosphere, fantasy art, high contrast lighting, epic composition' --size 1024x1024 --steps 35 --cfg-scale 8.5 --output '$WORKFLOW_DIR/dragon_detailed.png'" \
    "$WORKFLOW_DIR/dragon_detailed.png"

run_image_command \
    "Using artistic style references" \
    "agentflow image generate 'Portrait of a woman in renaissance style, Leonardo da Vinci technique, sfumato lighting, classical composition, oil painting texture, museum quality, detailed facial features' --size 768x768 --steps 40 --cfg-scale 9.0 --output '$WORKFLOW_DIR/portrait_renaissance.png'" \
    "$WORKFLOW_DIR/portrait_renaissance.png"

run_image_command \
    "Using mood and atmosphere keywords" \
    "agentflow image generate 'Abandoned space station drifting in deep space, eerie silence, dim emergency lighting, rust and decay, melancholic atmosphere, science fiction, cinematic composition, emotional storytelling' --size 1280x800 --steps 35 --cfg-scale 8.0 --output '$WORKFLOW_DIR/spacestation_moody.png'" \
    "$WORKFLOW_DIR/spacestation_moody.png"

# Workflow 4: Image Understanding and Analysis
workflow_step "4" "Professional Image Analysis Workflows"

explain_technique "Use AI vision to analyze images professionally. Learn different analysis approaches for various use cases."

run_image_command \
    "Technical composition analysis" \
    "agentflow image understand '$WORKFLOW_DIR/dragon_detailed.png' 'Analyze this image from a photographer perspective: composition, lighting, color balance, focal points, rule of thirds, depth of field, and overall technical quality.' --temperature 0.3 --max-tokens 800 --output '$WORKFLOW_DIR/dragon_technical_analysis.txt'" \
    "$WORKFLOW_DIR/dragon_technical_analysis.txt"

show_results "$WORKFLOW_DIR/dragon_technical_analysis.txt" "Technical Analysis"

run_image_command \
    "Artistic and emotional analysis" \
    "agentflow image understand '$WORKFLOW_DIR/portrait_renaissance.png' 'Analyze this portrait from an art historian perspective: artistic style, historical context, emotional expression, symbolic elements, brushwork technique, and cultural significance.' --temperature 0.7 --max-tokens 600 --output '$WORKFLOW_DIR/portrait_art_analysis.txt'" \
    "$WORKFLOW_DIR/portrait_art_analysis.txt"

show_results "$WORKFLOW_DIR/portrait_art_analysis.txt" "Artistic Analysis"

run_image_command \
    "Storytelling and narrative analysis" \
    "agentflow image understand '$WORKFLOW_DIR/spacestation_moody.png' 'Create a compelling backstory for this scene: What happened here? Who lived here? What events led to this moment? Write it as a movie synopsis.' --temperature 0.9 --max-tokens 500 --output '$WORKFLOW_DIR/spacestation_story.txt'" \
    "$WORKFLOW_DIR/spacestation_story.txt"

show_results "$WORKFLOW_DIR/spacestation_story.txt" "Story Creation"

# Workflow 5: Size and Format Optimization
workflow_step "5" "Size and Format Optimization"

explain_technique "Learn when to use different image sizes and formats for various applications."

run_image_command \
    "Square format for social media" \
    "agentflow image generate 'Minimalist coffee cup with steam, warm lighting, cozy atmosphere, Instagram aesthetic' --size 512x512 --steps 25 --output '$WORKFLOW_DIR/coffee_square.png'" \
    "$WORKFLOW_DIR/coffee_square.png"

run_image_command \
    "Wide format for desktop wallpaper" \
    "agentflow image generate 'Panoramic view of northern lights over snowy mountains, vibrant aurora colors, starry sky, wide composition' --size 1280x800 --steps 35 --output '$WORKFLOW_DIR/aurora_wide.png'" \
    "$WORKFLOW_DIR/aurora_wide.png"

run_image_command \
    "Portrait format for mobile wallpaper" \
    "agentflow image generate 'Serene zen garden with bamboo fountain, vertical composition, peaceful atmosphere, mobile wallpaper style' --size 512x768 --steps 30 --output '$WORKFLOW_DIR/zen_portrait.png'" \
    "$WORKFLOW_DIR/zen_portrait.png"

explain_technique "Choose sizes based on intended use:"
echo "  • 512x512: Social media, avatars, quick tests"
echo "  • 768x768: Detailed artwork, prints"
echo "  • 1024x1024: High quality, professional use"
echo "  • 1280x800: Wallpapers, wide displays"
echo "  • Custom ratios: Specific design needs"

# Workflow 6: Batch Processing and Iterations
workflow_step "6" "Batch Processing and Iterative Refinement"

explain_technique "Learn to create multiple variations efficiently and refine concepts through iteration."

BATCH_CONCEPT="A magical forest clearing with glowing mushrooms"

echo -e "${CYAN}🔄 Creating multiple variations for concept exploration:${NC}"

for i in {1..3}; do
    SEED=$((200 + i))
    run_image_command \
        "Variation $i with seed $SEED" \
        "agentflow image generate '$BATCH_CONCEPT, fantasy art, mystical atmosphere, detailed environment' --size 768x768 --steps 30 --cfg-scale 7.5 --seed $SEED --output '$WORKFLOW_DIR/forest_var_$i.png'" \
        "$WORKFLOW_DIR/forest_var_$i.png"
done

explain_technique "Batch processing allows you to:"
echo "  • Explore multiple interpretations quickly"
echo "  • A/B test different approaches"
echo "  • Build content libraries efficiently"
echo "  • Find the perfect result among variations"

# Workflow 7: Quality Assessment and Selection
workflow_step "7" "Quality Assessment Using AI"

explain_technique "Use AI vision to evaluate and compare your generated images objectively."

run_image_command \
    "Compare and rank the forest variations" \
    "agentflow image understand '$WORKFLOW_DIR/forest_var_1.png' 'Rate this image on: artistic quality (1-10), composition (1-10), color harmony (1-10), detail level (1-10), and overall appeal (1-10). Explain your ratings.' --temperature 0.2 --output '$WORKFLOW_DIR/forest_evaluation_1.txt'" \
    "$WORKFLOW_DIR/forest_evaluation_1.txt"

# Create a comprehensive comparison
echo -e "${CYAN}🏆 Creating comprehensive image comparison report:${NC}"

COMPARISON_PROMPT="Compare the artistic qualities of all three forest images. Which has the best composition, lighting, detail, and overall appeal? Rank them 1st, 2nd, 3rd with detailed justification."

# Note: This would ideally analyze multiple images, but we'll demonstrate the concept
run_image_command \
    "Generate quality comparison report" \
    "agentflow image understand '$WORKFLOW_DIR/forest_var_2.png' '$COMPARISON_PROMPT' --temperature 0.4 --max-tokens 600 --output '$WORKFLOW_DIR/forest_comparison_report.txt'" \
    "$WORKFLOW_DIR/forest_comparison_report.txt"

show_results "$WORKFLOW_DIR/forest_comparison_report.txt" "Quality Comparison Report"

# Workshop completion
echo ""
echo -e "${BOLD}${MAGENTA}🎨 Image Workflows Mastery Complete!${NC}"
echo "=========================================="
echo ""
echo "You've mastered advanced image workflows:"
echo -e "${GREEN}✅ Style variation techniques${NC}"
echo -e "${GREEN}✅ Parameter optimization strategies${NC}"
echo -e "${GREEN}✅ Professional prompting methods${NC}"
echo -e "${GREEN}✅ Multi-perspective image analysis${NC}"
echo -e "${GREEN}✅ Size and format optimization${NC}"
echo -e "${GREEN}✅ Batch processing workflows${NC}"
echo -e "${GREEN}✅ AI-powered quality assessment${NC}"
echo ""
echo "📁 Your image workflow outputs: $WORKFLOW_DIR"
echo ""
echo -e "${CYAN}🚀 Professional Tips:${NC}"
echo "  • Always use consistent seeds when A/B testing parameters"
echo "  • Combine technical prompts with artistic style keywords"
echo "  • Use temperature 0.2-0.4 for technical analysis, 0.7-0.9 for creative work"
echo "  • Build a library of successful prompt formulas for reuse"
echo "  • Use AI analysis to learn what makes images successful"
echo ""
echo -e "${CYAN}🎯 Advanced Techniques to Explore:${NC}"
echo "  • Seasonal and time-of-day variations"
echo "  • Character consistency across multiple images"
echo "  • Architecture and environment design workflows"
echo "  • Product visualization and marketing imagery"
echo ""
echo -e "${YELLOW}Next: Try './tutorials/03_audio_workflows.sh' for audio mastery!${NC}"
echo ""
echo -e "${BOLD}You're now ready for professional image generation workflows! 🎨✨${NC}"