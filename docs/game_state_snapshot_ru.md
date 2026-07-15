# Снэпшот состояния игры (GET /games/{id}/state)

Эндпоинт `GET /games/{id}/state` позволяет клиенту (фронтенду) в любой момент получить полный снимок текущего состояния игры. Это критически важно для восстановления состояния (Recovery) при перезагрузке страницы, обрыве WebSocket-соединения или при позднем входе игрока в лобби.

Эндпоинт возвращает структуру `GameStateDto`, которая меняется в зависимости от фазы игры и текущего раунда.

---

## Структура GameStateDto

Снимок состояния состоит из следующих основных блоков:

```json
{
  "status": 200,
  "message": "success",
  "data": {
    "game": {
      "id": "uuid",
      "mode": "situation_to_meme | meme_to_situation",
      "status": "lobby | playing | finished",
      "version": 42
    },
    "round": null, // присутствует только если game.status == "playing"
    "players": [
      {
        "user_id": "uuid",
        "score": 0,
        "is_ready": true,
        "handle": "username",
        "has_submitted": false
      }
    ],
    "my_hand": [ // активные карты в руке текущего игрока
      {
        "type": "meme",
        "id": "uuid",
        "media_url": "https://..."
      }
    ]
  }
}
```

### Основные поля:
- **`game`**: Базовая информация об игре (её ID, режим игры, статус и версия для оптимистичной блокировки).
- **`round`**: Состояние текущего активного раунда (подробнее ниже). Если игра находится в статусе `lobby`, данное поле равно `null`.
- **`players`**: Список участников лобби с их счетом, готовностью, игровым handle (ником) и флагом `has_submitted` (подал ли игрок карту в текущем раунде).
- **`my_hand`**: Список доступных карт в руке запрашивающего игрока. Использованные карты сюда не попадают.

---

## Детальная структура RoundDto

Поле `round` описывает текущий раунд и содержит новые поля для бесшовного восстановления интерфейса:

```typescript
interface RoundDto {
  id: string;                      // ID раунда (UUID)
  round_number: number;            // Номер текущего раунда (начиная с 1)
  phase: "waiting" | "submitting" | "voting" | "finished"; // Фаза раунда
  prompt: GameCard | null;         // Карта-промт раунда (ситуация или мем)
  phase_expires_at: string | null; // ISO дата/время истечения текущей фазы по таймеру
  
  // --- Новые поля восстановления состояния ---
  submissions?: RoundSubmissionDto[]; // Список поданных карт для голосования
  my_submission?: GameCard;           // Карточка, которую подал текущий игрок
  has_voted: boolean;                 // Проголосовал ли уже текущий игрок
}
```

---

## Состояние игры в зависимости от фаз раунда

### 1. Phase `submitting` (Сбор карт от игроков)
В этой фазе игроки выбирают карту из руки (`my_hand`) и отправляют её на сервер.

- **`submissions`**: **Отсутствует в ответе (или равен `null`)**. Это гарантирует секретность ходов: игроки не могут подсмотреть чужие отправленные карты до тех пор, пока фаза раунда не сменится на `voting`.
- **`my_submission`**: Если текущий игрок уже сделал ход, здесь вернется его отправленная карта. Клиент может использовать её, чтобы показать: *«Вы отправили карту: [Картинка/Текст]. Ожидаем остальных игроков...»*.
- **`players[i].has_submitted`**: Показывает статус готовности ходов всех игроков в лобби, помогая клиенту отрендерить список игроков с отметками «сходил / думает».

**Пример ответа на фазе `submitting`:**
```json
{
  "status": 200,
  "message": "success",
  "data": {
    "game": {
      "id": "d34415e0-2573-4213-b0d3-b706fb1f770b",
      "mode": "situation_to_meme",
      "status": "playing",
      "version": 5
    },
    "round": {
      "id": "8a250ac0-02e4-47b0-90e6-fa28e011f52e",
      "round_number": 1,
      "phase": "submitting",
      "prompt": {
        "type": "situation",
        "id": "e229e0b1-2037-48f8-8522-e215796c65b1",
        "prompt_text": "Когда тимлид сказал переписать все на Rust за выходные"
      },
      "phase_expires_at": "2026-07-15T19:50:00.000Z",
      "my_submission": {
        "type": "meme",
        "id": "a1dcd146-a08e-43aa-bbcf-f7db3c3c7c65",
        "media_url": "https://cdn.hackclub.com/meme1.jpg"
      },
      "has_voted": false
      // Поле "submissions" отсутствует во время фазы submitting (это секрет)
    },
    "players": [
      { "user_id": "e879e011-2037-48f8-8522-e215796c65b1", "score": 0, "is_ready": true, "handle": "Alice", "has_submitted": true },
      { "user_id": "04a1a398-d926-420c-b6be-46c088c205f2", "score": 0, "is_ready": true, "handle": "Bob", "has_submitted": false }
    ],
    "my_hand": [
      { "type": "meme", "id": "b78999f6-1f5d-4391-8cb6-0024ee231d8d", "media_url": "https://cdn.hackclub.com/meme2.jpg" }
    ]
  }
}
```

---

### 2. Фаза `voting` (Голосование за лучшую карту)
Фаза наступает автоматически, когда все игроки сделали ход.

- **`submissions`**: Содержит список **всех поданных карт** от игроков. 
  - **Важно:** Список полностью анонимизирован (в DTO нет поля `user_id`), чтобы исключить голосование за друзей.
  - Каждая запись содержит `id` сабмишена (по которому нужно слать голос на `POST /games/{id}/vote`) и объект `card`.
- **`my_submission`**: Возвращает карту текущего игрока. Клиент должен использовать её, чтобы **визуально выделить** карту игрока и **заблокировать кнопку голосования за самого себя** (так как самострелы запрещены правилами игры и вернут `400 Bad Request` от API).
- **`has_voted`**: Флаг показывает, отдал ли уже текущий игрок свой голос в этом раунде. Если `true`, фронтенд может сразу заблокировать интерактив голосования и показать экран ожидания результатов.

**Пример ответа на фазе `voting`:**
```json
{
  "status": 200,
  "message": "success",
  "data": {
    "game": {
      "id": "d34415e0-2573-4213-b0d3-b706fb1f770b",
      "mode": "situation_to_meme",
      "status": "playing",
      "version": 8
    },
    "round": {
      "id": "8a250ac0-02e4-47b0-90e6-fa28e011f52e",
      "round_number": 1,
      "phase": "voting",
      "prompt": {
        "type": "situation",
        "id": "e229e0b1-2037-48f8-8522-e215796c65b1",
        "prompt_text": "Когда тимлид сказал переписать все на Rust за выходные"
      },
      "phase_expires_at": "2026-07-15T19:52:30.000Z",
      "submissions": [
        {
          "id": "11111111-1111-1111-1111-111111111111", // ID сабмишена для отправки голоса
          "card": { "type": "meme", "id": "a1dcd146-a08e-43aa-bbcf-f7db3c3c7c65", "media_url": "https://cdn.hackclub.com/meme1.jpg" }
        },
        {
          "id": "22222222-2222-2222-2222-222222222222",
          "card": { "type": "meme", "id": "cccccccc-cccc-cccc-cccc-cccccccccccc", "media_url": "https://cdn.hackclub.com/meme3.jpg" }
        }
      ],
      "my_submission": {
        "type": "meme",
        "id": "a1dcd146-a08e-43aa-bbcf-f7db3c3c7c65",
        "media_url": "https://cdn.hackclub.com/meme1.jpg"
      },
      "has_voted": false
    },
    "players": [
      { "user_id": "e879e011-2037-48f8-8522-e215796c65b1", "score": 0, "is_ready": true, "handle": "Alice", "has_submitted": true },
      { "user_id": "04a1a398-d926-420c-b6be-46c088c205f2", "score": 0, "is_ready": true, "handle": "Bob", "has_submitted": true }
    ],
    "my_hand": [
      { "type": "meme", "id": "b78999f6-1f5d-4391-8cb6-0024ee231d8d", "media_url": "https://cdn.hackclub.com/meme2.jpg" }
    ]
  }
}
```

---

### 3. Фаза `finished` (Подведение итогов раунда)
Эта фаза длится короткое время после завершения голосования перед переходом к следующему раунду.

- **`submissions`**: Доступен список всех поданных карт.
- **`my_submission`**: Карточка игрока.
- **`has_voted`**: Всегда `true`.
- **`players`**: Оценки и счета игроков обновлены с учетом очков за этот раунд.
