package main

import (
	"database/sql"
	"errors"
	"fmt"
	"log"
	"mime"
	"os"
	"strings"
	"time"

	"github.com/gofiber/fiber/v2"
	"github.com/golang-jwt/jwt/v5"
	_ "github.com/lib/pq"
)

type postInput struct {
	Title string `json:"title"`
	Body  string `json:"body"`
}

type post struct {
	ID    int32  `json:"id"`
	Title string `json:"title"`
	Body  string `json:"body"`
}

type loginInput struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

func apiError(c *fiber.Ctx, status int, code string) error {
	return c.Status(status).JSON(fiber.Map{"error": fiber.Map{"code": code}})
}

func parsePost(c *fiber.Ctx) (postInput, error) {
	var input postInput
	if err := c.BodyParser(&input); err != nil || len(input.Title) == 0 || len([]rune(input.Title)) > 120 || len(input.Body) == 0 {
		return input, errors.New("invalid post input")
	}
	return input, nil
}

func hasJSONContentType(value string) bool {
	mediaType, _, err := mime.ParseMediaType(value)
	return err == nil && strings.EqualFold(mediaType, "application/json")
}

func bearerToken(value string) (string, bool) {
	parts := strings.Fields(value)
	if len(parts) != 2 || !strings.EqualFold(parts[0], "Bearer") {
		return "", false
	}
	return parts[1], true
}

func postsApp(app *fiber.App, db *sql.DB) {
	app.Post("/posts", func(c *fiber.Ctx) error {
		if !hasJSONContentType(c.Get(fiber.HeaderContentType)) {
			return apiError(c, fiber.StatusUnsupportedMediaType, "unsupported_media_type")
		}
		input, err := parsePost(c)
		if err != nil {
			return apiError(c, fiber.StatusUnprocessableEntity, "validation_error")
		}
		item := post{Title: input.Title, Body: input.Body}
		if err := db.QueryRow("INSERT INTO posts (title, body) VALUES ($1, $2) RETURNING id", item.Title, item.Body).Scan(&item.ID); err != nil {
			return apiError(c, fiber.StatusInternalServerError, "internal")
		}
		return c.Status(fiber.StatusCreated).JSON(item)
	})
	app.Get("/posts", func(c *fiber.Ctx) error {
		rows, err := db.Query("SELECT id, title, body FROM posts ORDER BY id")
		if err != nil {
			return apiError(c, fiber.StatusInternalServerError, "internal")
		}
		defer rows.Close()
		items := make([]post, 0)
		for rows.Next() {
			var item post
			if err := rows.Scan(&item.ID, &item.Title, &item.Body); err != nil {
				return apiError(c, fiber.StatusInternalServerError, "internal")
			}
			items = append(items, item)
		}
		if err := rows.Err(); err != nil {
			return apiError(c, fiber.StatusInternalServerError, "internal")
		}
		return c.JSON(items)
	})
	app.Get("/posts/:id", func(c *fiber.Ctx) error {
		id, err := c.ParamsInt("id")
		if err != nil {
			return apiError(c, fiber.StatusNotFound, "not_found")
		}
		var item post
		err = db.QueryRow("SELECT id, title, body FROM posts WHERE id = $1", id).Scan(&item.ID, &item.Title, &item.Body)
		if errors.Is(err, sql.ErrNoRows) {
			return apiError(c, fiber.StatusNotFound, "not_found")
		}
		if err != nil {
			return apiError(c, fiber.StatusInternalServerError, "internal")
		}
		return c.JSON(item)
	})
	app.Put("/posts/:id", func(c *fiber.Ctx) error {
		id, err := c.ParamsInt("id")
		if err != nil {
			return apiError(c, fiber.StatusNotFound, "not_found")
		}
		if !hasJSONContentType(c.Get(fiber.HeaderContentType)) {
			return apiError(c, fiber.StatusUnsupportedMediaType, "unsupported_media_type")
		}
		input, err := parsePost(c)
		if err != nil {
			return apiError(c, fiber.StatusUnprocessableEntity, "validation_error")
		}
		item := post{Title: input.Title, Body: input.Body}
		err = db.QueryRow("UPDATE posts SET title = $1, body = $2 WHERE id = $3 RETURNING id", item.Title, item.Body, id).Scan(&item.ID)
		if errors.Is(err, sql.ErrNoRows) {
			return apiError(c, fiber.StatusNotFound, "not_found")
		}
		if err != nil {
			return apiError(c, fiber.StatusInternalServerError, "internal")
		}
		return c.JSON(item)
	})
	app.Delete("/posts/:id", func(c *fiber.Ctx) error {
		id, err := c.ParamsInt("id")
		if err != nil {
			return apiError(c, fiber.StatusNotFound, "not_found")
		}
		result, err := db.Exec("DELETE FROM posts WHERE id = $1", id)
		if err != nil {
			return apiError(c, fiber.StatusInternalServerError, "internal")
		}
		rows, err := result.RowsAffected()
		if err != nil {
			return apiError(c, fiber.StatusInternalServerError, "internal")
		}
		if rows == 0 {
			return apiError(c, fiber.StatusNotFound, "not_found")
		}
		return c.SendStatus(fiber.StatusNoContent)
	})
}

func authApp(app *fiber.App, username, password, secret string) {
	app.Post("/auth/login", func(c *fiber.Ctx) error {
		if !hasJSONContentType(c.Get(fiber.HeaderContentType)) {
			return apiError(c, fiber.StatusUnsupportedMediaType, "unsupported_media_type")
		}
		var input loginInput
		if err := c.BodyParser(&input); err != nil || input.Username == "" || len([]rune(input.Password)) < 8 {
			return apiError(c, fiber.StatusUnprocessableEntity, "validation_error")
		}
		if input.Username != username || input.Password != password {
			log.Print("login rejected")
			return apiError(c, fiber.StatusUnauthorized, "unauthorized")
		}
		token := jwt.NewWithClaims(jwt.SigningMethodHS256, jwt.MapClaims{"sub": "1", "user_id": 1, "exp": time.Now().Add(15 * time.Minute).Unix()})
		signed, err := token.SignedString([]byte(secret))
		if err != nil {
			return apiError(c, fiber.StatusInternalServerError, "internal")
		}
		log.Print("login succeeded")
		return c.JSON(fiber.Map{"access_token": signed})
	})
	app.Get("/auth/me", func(c *fiber.Ctx) error {
		rawToken, ok := bearerToken(c.Get("Authorization"))
		if !ok {
			return passportUnauthorized(c)
		}
		token, err := jwt.Parse(rawToken, func(token *jwt.Token) (interface{}, error) {
			if token.Method.Alg() != jwt.SigningMethodHS256.Alg() {
				return nil, errors.New("unexpected JWT algorithm")
			}
			return []byte(secret), nil
		})
		if err != nil || !token.Valid {
			return passportUnauthorized(c)
		}
		claims, ok := token.Claims.(jwt.MapClaims)
		if !ok || claims["sub"] != "1" || claims["user_id"] != float64(1) {
			return passportUnauthorized(c)
		}
		log.Print("protected profile read")
		return c.JSON(fiber.Map{"id": 1, "username": username})
	})
}

func passportUnauthorized(c *fiber.Ctx) error {
	c.Set("WWW-Authenticate", "Bearer")
	return c.Status(fiber.StatusUnauthorized).JSON(fiber.Map{
		"error": fiber.Map{"code": "unauthorized", "message": "authentication was rejected"},
	})
}

func main() {
	mode := os.Getenv("BENCH_MODE")
	port := os.Getenv("PORT")
	if port == "" {
		log.Fatal("PORT required")
	}
	app := fiber.New(fiber.Config{DisableStartupMessage: true, BodyLimit: 2 * 1024 * 1024})
	switch mode {
	case "hello":
		app.Get("/", func(c *fiber.Ctx) error { return c.SendString("Hello, world!") })
	case "posts":
		url := os.Getenv("DATABASE_URL")
		if url == "" {
			log.Fatal("DATABASE_URL required")
		}
		db, err := sql.Open("postgres", url)
		if err != nil {
			log.Fatal(err)
		}
		defer db.Close()
		db.SetMaxOpenConns(10)
		db.SetMaxIdleConns(10)
		if err := db.Ping(); err != nil {
			log.Fatal(err)
		}
		postsApp(app, db)
	case "auth":
		authApp(app, os.Getenv("DEMO_USERNAME"), os.Getenv("DEMO_PASSWORD"), os.Getenv("JWT_SECRET"))
	default:
		log.Fatal("BENCH_MODE must be hello, posts, or auth")
	}
	if err := app.Listen(fmt.Sprintf("127.0.0.1:%s", port)); err != nil {
		log.Fatal(err)
	}
}
