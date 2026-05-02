package api

import (
	"embed"
	"io/fs"
	"net/http"

	"github.com/abdullaabdullazade/freedrive/internal/api/handlers"
	"github.com/abdullaabdullazade/freedrive/internal/api/middleware"
	"github.com/abdullaabdullazade/freedrive/internal/repository"
	"github.com/abdullaabdullazade/freedrive/internal/service"
	"github.com/abdullaabdullazade/freedrive/internal/storage"
	"github.com/go-chi/chi/v5"
	chiMiddleware "github.com/go-chi/chi/v5/middleware"
)

// NewRouter creates and configures the HTTP router.
func NewRouter(
	webFS embed.FS,
	authService *service.AuthService,
	fileService *service.FileService,
	folderService *service.FolderService,
	fileRepo repository.FileRepository,
	userRepo repository.UserRepository,
	activityRepo repository.ActivityRepository,
	diskStorage *storage.DiskStorage,
	maxUpload int64,
) http.Handler {
	r := chi.NewRouter()

	// Global middleware
	r.Use(chiMiddleware.RequestID)
	r.Use(chiMiddleware.RealIP)
	r.Use(chiMiddleware.Logger)
	r.Use(chiMiddleware.Recoverer)
	r.Use(chiMiddleware.Compress(5))
	r.Use(middleware.CORS)

	// Rate limiter: 100 requests/second, burst of 200
	limiter := middleware.NewRateLimiter(100, 200)
	r.Use(limiter.Limit)

	// Create handlers
	authHandler := handlers.NewAuthHandler(authService)
	fileHandler := handlers.NewFileHandler(fileService, fileRepo, diskStorage, maxUpload)
	folderHandler := handlers.NewFolderHandler(folderService)
	adminHandler := handlers.NewAdminHandler(userRepo, fileRepo, activityRepo, authService)
	userHandler := handlers.NewUserHandler(userRepo)

	// API routes
	r.Route("/api/v1", func(r chi.Router) {
		// Public auth routes
		r.Route("/auth", func(r chi.Router) {
			r.Post("/register", authHandler.Register)
			r.Post("/login", authHandler.Login)
			r.Post("/refresh", authHandler.Refresh)
			r.Post("/logout", authHandler.Logout)
			r.Post("/reset-password", authHandler.ResetPassword)
		})

		// Protected routes
		r.Group(func(r chi.Router) {
			r.Use(middleware.Auth(authService))

			// User storage (quota-based)
			r.Get("/me/storage", userHandler.MyStorage)

			// Files
			r.Route("/files", func(r chi.Router) {
				r.Post("/upload", fileHandler.Upload)
				r.Get("/", fileHandler.List)
				r.Get("/trash", fileHandler.Trash)
				r.Get("/{id}", fileHandler.Get)
				r.Get("/{id}/download", fileHandler.Download)
				r.Patch("/{id}", fileHandler.Update)
				r.Post("/{id}/content", fileHandler.UpdateContent)
				r.Delete("/{id}", fileHandler.Delete)
				r.Post("/{id}/restore", fileHandler.Restore)
				r.Delete("/{id}/permanent", fileHandler.PermanentDelete)
				r.Get("/{id}/versions", fileHandler.GetVersions)
				r.Post("/{id}/versions/{version}/restore", fileHandler.RestoreVersion)
			})

			// Folders
			r.Route("/folders", func(r chi.Router) {
				r.Post("/", folderHandler.Create)
				r.Get("/root", folderHandler.GetRoot)
				r.Get("/{id}", folderHandler.Get)
				r.Patch("/{id}", folderHandler.Update)
				r.Delete("/{id}", folderHandler.Delete)
				r.Get("/{id}/breadcrumb", folderHandler.GetBreadcrumb)
			})

			// Activity
			r.Get("/activity", func(w http.ResponseWriter, r *http.Request) {
				adminHandler.MyActivity(w, r)
			})

			// Disk stats — real system disk usage
			r.Get("/disk-stats", handlers.DiskStats)

			// Admin routes
			r.Route("/admin", func(r chi.Router) {
				r.Use(middleware.RequireAdmin)

				r.Get("/users", adminHandler.ListUsers)
				r.Post("/users", adminHandler.CreateUser)
				r.Patch("/users/{id}", adminHandler.UpdateUser)
				r.Delete("/users/{id}", adminHandler.DeleteUser)
				r.Post("/users/{id}/reset-password", adminHandler.SendPasswordReset)
				r.Get("/stats", adminHandler.Stats)
				r.Post("/invites", adminHandler.CreateInvite)
				r.Post("/invites/resend", adminHandler.ResendInvite)
				r.Get("/invites", adminHandler.ListInvites)
				r.Get("/activity", adminHandler.Activity)
				r.Get("/settings", adminHandler.GetSettings)
				r.Post("/settings", adminHandler.SaveSettings)
				r.Post("/test-email", adminHandler.TestEmail)
				r.Post("/backup/run", adminHandler.RunBackupNow)
			})
		})

		// Health check
		r.Get("/health", func(w http.ResponseWriter, r *http.Request) {
			w.Header().Set("Content-Type", "application/json")
			w.Write([]byte(`{"status":"ok"}`))
		})
	})

	// Serve embedded frontend
	webRoot, err := fs.Sub(webFS, "web")
	if err != nil {
		panic("failed to get web sub filesystem: " + err.Error())
	}
	fileServer := http.FileServer(http.FS(webRoot))

	// SPA fallback: serve index.html for all non-API, non-static routes
	r.Get("/*", func(w http.ResponseWriter, r *http.Request) {
		// Try to serve static file first
		path := r.URL.Path
		if path == "/" {
			path = "/index.html"
		}

		// Check if file exists in embedded FS
		f, err := webRoot.(fs.ReadFileFS).ReadFile(path[1:]) // strip leading /
		if err != nil {
			// Serve index.html for SPA routes
			f, err = webRoot.(fs.ReadFileFS).ReadFile("index.html")
			if err != nil {
				http.NotFound(w, r)
				return
			}
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
			w.Write(f)
			return
		}

		// Detect content type
		_ = f
		fileServer.ServeHTTP(w, r)
	})

	return r
}
