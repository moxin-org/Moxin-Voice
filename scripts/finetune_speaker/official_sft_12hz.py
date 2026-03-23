#!/usr/bin/env python3
# coding=utf-8
"""
Official-style SFT for Qwen3-TTS-12Hz Base model.

Adapted from Qwen3-TTS/finetuning/sft_12hz.py with:
- configurable speaker_id
- row update/append safety for codec_embedding
- clearer logs for local moxin-tts workflow
"""

import argparse
import json
import os
import shutil

import torch
from accelerate import Accelerator
from official_dataset import TTSDataset
from qwen_tts.inference.qwen3_tts_model import Qwen3TTSModel
from safetensors.torch import save_file
from torch.optim import AdamW
from torch.utils.data import DataLoader
from transformers import AutoConfig


def patch_codec_embedding_row(state_dict, speaker_embedding: torch.Tensor, speaker_id: int) -> None:
    key = "talker.model.codec_embedding.weight"
    weight = state_dict[key]
    rows, dim = weight.shape
    emb = speaker_embedding.detach().to(weight.device).to(weight.dtype).view(1, dim)

    if speaker_id < rows:
        weight[speaker_id] = emb[0]
        print(f"[sft] codec_embedding: update row {speaker_id} in ({rows}, {dim})")
        return
    if speaker_id == rows:
        state_dict[key] = torch.cat([weight, emb], dim=0)
        print(f"[sft] codec_embedding: append row {speaker_id} ({rows}, {dim}) -> ({rows + 1}, {dim})")
        return

    raise ValueError(f"speaker_id={speaker_id} exceeds rows={rows}; must be <= {rows}")


def train() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--init_model_path", type=str, default="Qwen/Qwen3-TTS-12Hz-1.7B-Base")
    parser.add_argument("--output_model_path", type=str, default="output")
    parser.add_argument("--train_jsonl", type=str, required=True)
    parser.add_argument("--batch_size", type=int, default=2)
    parser.add_argument("--lr", type=float, default=2e-5)
    parser.add_argument("--num_epochs", type=int, default=3)
    parser.add_argument("--speaker_name", type=str, default="speaker_test")
    parser.add_argument("--speaker_id", type=int, default=3000)
    parser.add_argument("--grad_accum_steps", type=int, default=4)
    parser.add_argument("--device_map", type=str, default="cuda:0")
    parser.add_argument(
        "--dtype",
        type=str,
        default="bfloat16",
        choices=["bfloat16", "float16", "float32"],
    )
    parser.add_argument("--attn_implementation", type=str, default="flash_attention_2")
    args = parser.parse_args()

    accelerator = Accelerator(
        gradient_accumulation_steps=args.grad_accum_steps,
        mixed_precision="bf16",
        log_with="tensorboard",
    )

    model_path = args.init_model_path
    dtype_map = {
        "bfloat16": torch.bfloat16,
        "float16": torch.float16,
        "float32": torch.float32,
    }
    qwen3tts = Qwen3TTSModel.from_pretrained(
        model_path,
        device_map=args.device_map,
        torch_dtype=dtype_map[args.dtype],
        attn_implementation=args.attn_implementation,
    )
    config = AutoConfig.from_pretrained(model_path)

    with open(args.train_jsonl, "r", encoding="utf-8") as f:
        train_data = [json.loads(line) for line in f if line.strip()]

    dataset = TTSDataset(train_data, qwen3tts.processor, config)
    train_dataloader = DataLoader(dataset, batch_size=args.batch_size, shuffle=True, collate_fn=dataset.collate_fn)

    optimizer = AdamW(qwen3tts.model.parameters(), lr=args.lr, weight_decay=0.01)
    model, optimizer, train_dataloader = accelerator.prepare(qwen3tts.model, optimizer, train_dataloader)

    model.train()
    target_speaker_embedding = None

    for epoch in range(args.num_epochs):
        for step, batch in enumerate(train_dataloader):
            with accelerator.accumulate(model):
                input_ids = batch["input_ids"]
                codec_ids = batch["codec_ids"]
                ref_mels = batch["ref_mels"]
                text_embedding_mask = batch["text_embedding_mask"]
                codec_embedding_mask = batch["codec_embedding_mask"]
                attention_mask = batch["attention_mask"]
                codec_0_labels = batch["codec_0_labels"]
                codec_mask = batch["codec_mask"]

                speaker_embedding = model.speaker_encoder(ref_mels.to(model.device).to(model.dtype)).detach()
                if target_speaker_embedding is None:
                    target_speaker_embedding = speaker_embedding

                input_text_ids = input_ids[:, :, 0]
                input_codec_ids = input_ids[:, :, 1]

                input_text_embedding = model.talker.model.text_embedding(input_text_ids) * text_embedding_mask
                input_codec_embedding = model.talker.model.codec_embedding(input_codec_ids) * codec_embedding_mask
                input_codec_embedding[:, 6, :] = speaker_embedding

                input_embeddings = input_text_embedding + input_codec_embedding
                for i in range(1, 16):
                    codec_i_embedding = model.talker.code_predictor.get_input_embeddings()[i - 1](codec_ids[:, :, i])
                    codec_i_embedding = codec_i_embedding * codec_mask.unsqueeze(-1)
                    input_embeddings = input_embeddings + codec_i_embedding

                outputs = model.talker(
                    inputs_embeds=input_embeddings[:, :-1, :],
                    attention_mask=attention_mask[:, :-1],
                    labels=codec_0_labels[:, 1:],
                    output_hidden_states=True,
                )

                hidden_states = outputs.hidden_states[0][-1]
                talker_hidden_states = hidden_states[codec_mask[:, :-1]]
                talker_codec_ids = codec_ids[codec_mask]

                _, sub_talker_loss = model.talker.forward_sub_talker_finetune(talker_codec_ids, talker_hidden_states)
                loss = outputs.loss + 0.3 * sub_talker_loss

                accelerator.backward(loss)
                if accelerator.sync_gradients:
                    accelerator.clip_grad_norm_(model.parameters(), 1.0)
                optimizer.step()
                optimizer.zero_grad()

            if step % 10 == 0:
                accelerator.print(f"Epoch {epoch} | Step {step} | Loss: {loss.item():.4f}")

        if accelerator.is_main_process:
            output_dir = os.path.join(args.output_model_path, f"checkpoint-epoch-{epoch}")
            shutil.copytree(model_path, output_dir, dirs_exist_ok=True)

            input_config_file = os.path.join(model_path, "config.json")
            output_config_file = os.path.join(output_dir, "config.json")
            with open(input_config_file, "r", encoding="utf-8") as f:
                config_dict = json.load(f)

            config_dict["tts_model_type"] = "custom_voice"
            talker_config = config_dict.get("talker_config", {})
            talker_config["spk_id"] = {args.speaker_name: args.speaker_id}
            talker_config["spk_is_dialect"] = {args.speaker_name: False}
            config_dict["talker_config"] = talker_config

            with open(output_config_file, "w", encoding="utf-8") as f:
                json.dump(config_dict, f, indent=2, ensure_ascii=False)

            unwrapped_model = accelerator.unwrap_model(model)
            state_dict = {k: v.detach().to("cpu") for k, v in unwrapped_model.state_dict().items()}

            drop_prefix = "speaker_encoder"
            keys_to_drop = [k for k in state_dict.keys() if k.startswith(drop_prefix)]
            for k in keys_to_drop:
                del state_dict[k]

            if target_speaker_embedding is None:
                raise RuntimeError("target_speaker_embedding is None; no training step ran")

            patch_codec_embedding_row(state_dict, target_speaker_embedding[0], args.speaker_id)

            save_path = os.path.join(output_dir, "model.safetensors")
            save_file(state_dict, save_path)
            print(f"[sft] saved checkpoint -> {save_path}")


if __name__ == "__main__":
    train()
